import type { MapDocument } from './map';

/** Where the user is looking: image coordinates of the canvas's top-left corner and CSS pixels per image pixel. */
export interface Camera {
  x: number;
  y: number;
  k: number;
}

/** Counters for measuring; not used by the app itself. */
export const renderStats = { draws: 0, cpuMs: 0 };

const VERT = `#version 300 es
precision highp float;
uniform vec3 uView;   // image coords of the top-left corner, CSS px per image px
uniform vec2 uSize;   // canvas size in CSS px
uniform float uRadius; // outline copies are spread on a circle of this many CSS px
uniform int uCopies;
in vec2 aPos;
in uint aId;
flat out uint vId;
void main() {
  vec2 s = (aPos - uView.xy) * uView.z;
  if (uCopies > 1) {
    float a = 6.2831853 * float(gl_InstanceID) / float(uCopies);
    s += uRadius * vec2(cos(a), sin(a));
  }
  gl_Position = vec4(s.x / uSize.x * 2.0 - 1.0, 1.0 - s.y / uSize.y * 2.0, 0.0, 1.0);
  vId = aId;
}`;

// Selection and hover tints are baked into the palette by the document, so the shader only looks colors up.
const FILL_FRAG = `#version 300 es
precision highp float;
uniform sampler2D uPalette;
uniform int uPaletteWidth;
flat in uint vId;
out vec4 outColor;
void main() {
  int id = int(vId);
  outColor = vec4(texelFetch(uPalette, ivec2(id % uPaletteWidth, id / uPaletteWidth), 0).rgb, 1.0);
}`;

const LINE_FRAG = `#version 300 es
precision highp float;
uniform vec4 uColor;
flat in uint vId;
out vec4 outColor;
void main() { outColor = uColor; }`;

interface Program {
  prog: WebGLProgram;
  loc: Record<string, WebGLUniformLocation | null>;
}

/** A set of border segments on the GPU: one element buffer, drawn with the shared point buffer. */
interface LineSet {
  vao: WebGLVertexArrayObject;
  indices: WebGLBuffer;
  count: number;
}

export interface DrawOptions {
  /** Draw the state-view borders (if any were set) in a stronger line. */
  stateView: boolean;
}

export class MapRenderer {
  private gl: WebGL2RenderingContext;
  private fill: Program;
  private line: Program;
  private fillVao: WebGLVertexArrayObject;
  private fillBuffers: WebGLBuffer[] = [];
  private linePoints: WebGLBuffer | null = null;
  private allBorders?: LineSet;
  private stateBorders?: LineSet;
  private stateBordersActive = false;
  private hoverOutline?: LineSet;
  private selectionOutline?: LineSet;
  private palette: WebGLTexture;
  private paletteWidth = 1;
  private paletteRows = 1;
  private triIndices = 0;
  private hasMap = false;
  private cssW = 1;
  private cssH = 1;

  constructor(private canvas: HTMLCanvasElement) {
    const gl = canvas.getContext('webgl2', { antialias: true, alpha: false, powerPreference: 'high-performance' });
    if (!gl) throw new Error('WebGL2 is not available in this browser');
    this.gl = gl;
    this.fill = this.program(VERT, FILL_FRAG, ['uView', 'uSize', 'uRadius', 'uCopies', 'uPalette', 'uPaletteWidth']);
    this.line = this.program(VERT, LINE_FRAG, ['uView', 'uSize', 'uRadius', 'uCopies', 'uColor']);
    this.fillVao = gl.createVertexArray()!;
    this.palette = gl.createTexture()!;
  }

  private program(vs: string, fs: string, uniforms: string[]): Program {
    const gl = this.gl;
    const compile = (type: number, src: string) => {
      const sh = gl.createShader(type)!;
      gl.shaderSource(sh, src);
      gl.compileShader(sh);
      if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(sh) ?? 'shader error');
      return sh;
    };
    const prog = gl.createProgram()!;
    gl.attachShader(prog, compile(gl.VERTEX_SHADER, vs));
    gl.attachShader(prog, compile(gl.FRAGMENT_SHADER, fs));
    gl.bindAttribLocation(prog, 0, 'aPos');
    gl.bindAttribLocation(prog, 1, 'aId');
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(prog) ?? 'link error');
    return { prog, loc: Object.fromEntries(uniforms.map((u) => [u, gl.getUniformLocation(prog, u)])) };
  }

  private buffer(target: number, data: ArrayBufferView | null, usage: number): WebGLBuffer {
    const gl = this.gl;
    const b = gl.createBuffer()!;
    gl.bindBuffer(target, b);
    gl.bufferData(target, data ?? new Uint8Array(0), usage);
    return b;
  }

  /** A line set whose segments read the shared border-point buffer. */
  private lineSet(data: Uint32Array | null, usage: number): LineSet {
    const gl = this.gl;
    const vao = gl.createVertexArray()!;
    gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.linePoints);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.disableVertexAttribArray(1);
    gl.vertexAttribI4ui(1, 0, 0, 0, 0);
    const indices = this.buffer(gl.ELEMENT_ARRAY_BUFFER, data, usage);
    gl.bindVertexArray(null);
    return { vao, indices, count: data?.length ?? 0 };
  }

  private replace(set: LineSet | undefined, data: Uint32Array | null): LineSet {
    const gl = this.gl;
    if (!set) return this.lineSet(data, gl.DYNAMIC_DRAW);
    gl.bindVertexArray(set.vao);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, data ?? new Uint8Array(0), gl.DYNAMIC_DRAW);
    gl.bindVertexArray(null);
    set.count = data?.length ?? 0;
    return set;
  }

  private freeLineSet(set: LineSet | undefined) {
    if (!set) return;
    this.gl.deleteBuffer(set.indices);
    this.gl.deleteVertexArray(set.vao);
  }

  /** Upload a map's geometry. The document's buffer views point into WASM memory, so they are consumed right here. */
  setDocument(doc: MapDocument) {
    const gl = this.gl;
    for (const b of this.fillBuffers) gl.deleteBuffer(b);
    if (this.linePoints) gl.deleteBuffer(this.linePoints);
    for (const s of [this.allBorders, this.stateBorders, this.hoverOutline, this.selectionOutline]) this.freeLineSet(s);
    this.stateBorders = this.hoverOutline = this.selectionOutline = undefined;
    this.stateBordersActive = false;

    gl.bindVertexArray(this.fillVao);
    const pos = this.buffer(gl.ARRAY_BUFFER, doc.positions(), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    const ids = this.buffer(gl.ARRAY_BUFFER, doc.vertexProvince(), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribIPointer(1, 1, gl.UNSIGNED_INT, 0, 0);
    const indices = doc.indices();
    this.triIndices = indices.length;
    const idx = this.buffer(gl.ELEMENT_ARRAY_BUFFER, indices, gl.STATIC_DRAW);
    gl.bindVertexArray(null);
    this.fillBuffers = [pos, ids, idx];

    this.linePoints = this.buffer(gl.ARRAY_BUFFER, doc.linePositions(), gl.STATIC_DRAW);
    this.allBorders = this.lineSet(doc.lineIndices(), gl.STATIC_DRAW);

    // One palette texel per province, in rows of up to 4096. Filled by setPalette.
    const n = Math.max(doc.len, 1);
    this.paletteWidth = Math.min(n, 4096);
    this.paletteRows = Math.ceil(n / this.paletteWidth);
    gl.bindTexture(gl.TEXTURE_2D, this.palette);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, this.paletteWidth, this.paletteRows, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    this.hasMap = true;
  }

  /** New province colors: four bytes (RGBA) per province, from id 0. */
  setPalette(rgba: Uint8Array) {
    const gl = this.gl;
    const texels = new Uint8Array(this.paletteWidth * this.paletteRows * 4);
    texels.set(rgba.subarray(0, texels.length));
    gl.bindTexture(gl.TEXTURE_2D, this.palette);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, this.paletteWidth, this.paletteRows, gl.RGBA, gl.UNSIGNED_BYTE, texels);
  }

  /** Borders for the state view (segment index pairs), or null to have none. */
  setStateBorders(indices: Uint32Array | null) {
    this.stateBorders = this.replace(this.stateBorders, indices);
    this.stateBordersActive = indices !== null;
  }

  /** Outline (segment index pairs) drawn around hovered or selected provinces; null clears it. */
  setOutline(slot: 'hover' | 'selection', indices: Uint32Array | null) {
    if (slot === 'hover') this.hoverOutline = this.replace(this.hoverOutline, indices);
    else this.selectionOutline = this.replace(this.selectionOutline, indices);
  }

  resize(cssW: number, cssH: number, dpr: number) {
    this.cssW = Math.max(1, cssW);
    this.cssH = Math.max(1, cssH);
    this.canvas.width = Math.max(1, Math.round(this.cssW * dpr));
    this.canvas.height = Math.max(1, Math.round(this.cssH * dpr));
  }

  draw(cam: Camera, opts: DrawOptions) {
    const t0 = performance.now();
    const gl = this.gl;
    gl.viewport(0, 0, this.canvas.width, this.canvas.height);
    gl.clearColor(0.078, 0.086, 0.102, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    if (!this.hasMap) return;

    const setCommon = (p: Program, radius: number, copies: number) => {
      gl.uniform3f(p.loc.uView, cam.x, cam.y, cam.k);
      gl.uniform2f(p.loc.uSize, this.cssW, this.cssH);
      gl.uniform1f(p.loc.uRadius, radius);
      gl.uniform1i(p.loc.uCopies, copies);
    };

    // Fills.
    gl.disable(gl.BLEND);
    gl.useProgram(this.fill.prog);
    setCommon(this.fill, 0, 1);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.palette);
    gl.uniform1i(this.fill.loc.uPalette, 0);
    gl.uniform1i(this.fill.loc.uPaletteWidth, this.paletteWidth);
    gl.bindVertexArray(this.fillVao);
    gl.drawElements(gl.TRIANGLES, this.triIndices, gl.UNSIGNED_INT, 0);

    // Borders: every province border, or in the state view only the ones between states.
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    gl.useProgram(this.line.prog);
    const stateSet = opts.stateView && this.stateBordersActive ? this.stateBorders : undefined;
    const borders = stateSet ?? this.allBorders!;
    gl.bindVertexArray(borders.vao);
    setCommon(this.line, 0, 1);
    gl.uniform4f(this.line.loc.uColor, 0, 0, 0, stateSet ? 0.65 : 0.3);
    gl.drawElements(gl.LINES, borders.count, gl.UNSIGNED_INT, 0);

    // Outlines are 8 copies of the lines nudged around a circle: a thick line with plain 1px GL lines.
    const outline = (set: LineSet | undefined, r: number, g: number, b: number, radius: number) => {
      if (!set || set.count <= 0) return;
      gl.bindVertexArray(set.vao);
      setCommon(this.line, radius, 8);
      gl.uniform4f(this.line.loc.uColor, r, g, b, 1);
      gl.drawElementsInstanced(gl.LINES, set.count, gl.UNSIGNED_INT, 0, 8);
    };
    outline(this.hoverOutline, 1, 1, 1, 1);
    outline(this.selectionOutline, 1, 0.83, 0, 1.5);

    gl.bindVertexArray(null);
    renderStats.draws++;
    renderStats.cpuMs += performance.now() - t0;
  }

  /**
   * A small JPEG of the map as seen by `cam`, for thumbnails. Draws first and reads the
   * canvas in the same task, which is the only time a WebGL canvas can be read back.
   */
  snapshot(cam: Camera, mapW: number, mapH: number, maxW: number): string {
    this.draw(cam, { stateView: false });
    const dpr = this.canvas.width / this.cssW;
    const sx = -cam.x * cam.k * dpr;
    const sy = -cam.y * cam.k * dpr;
    const sw = mapW * cam.k * dpr;
    const sh = mapH * cam.k * dpr;
    const out = document.createElement('canvas');
    out.width = maxW;
    out.height = Math.max(1, Math.round((maxW * sh) / sw));
    out.getContext('2d')!.drawImage(this.canvas, sx, sy, sw, sh, 0, 0, out.width, out.height);
    return out.toDataURL('image/jpeg', 0.75);
  }

  dispose() {
    const gl = this.gl;
    for (const b of this.fillBuffers) gl.deleteBuffer(b);
    if (this.linePoints) gl.deleteBuffer(this.linePoints);
    for (const s of [this.allBorders, this.stateBorders, this.hoverOutline, this.selectionOutline]) this.freeLineSet(s);
    gl.deleteTexture(this.palette);
    gl.deleteVertexArray(this.fillVao);
    gl.deleteProgram(this.fill.prog);
    gl.deleteProgram(this.line.prog);
  }
}
