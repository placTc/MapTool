import type { MapMesh } from './map';

/** Where the user is looking: image coordinates of the canvas's top-left corner and CSS pixels per image pixel. */
export interface Camera {
  x: number;
  y: number;
  k: number;
}

/** Counters for measuring; not used by the app itself. */
export const renderStats = { draws: 0, cpuMs: 0 };

const NONE = 0xffffffff;

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

const FILL_FRAG = `#version 300 es
precision highp float;
uniform sampler2D uPalette;
uniform int uPaletteWidth;
uniform uint uHover;
uniform uint uSelected;
flat in uint vId;
out vec4 outColor;
void main() {
  int id = int(vId);
  vec3 c = texelFetch(uPalette, ivec2(id % uPaletteWidth, id / uPaletteWidth), 0).rgb;
  if (vId == uSelected) c = mix(c, vec3(1.0, 0.83, 0.0), 0.45);
  if (vId == uHover) c = mix(c, vec3(1.0), 0.35);
  outColor = vec4(c, 1.0);
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

export class MapRenderer {
  private gl: WebGL2RenderingContext;
  private fill: Program;
  private line: Program;
  private fillVao: WebGLVertexArrayObject;
  private lineVao: WebGLVertexArrayObject;
  private buffers: WebGLBuffer[] = [];
  private palette: WebGLTexture;
  private paletteWidth = 1;
  private triIndices = 0;
  private lineIndices = 0;
  private mesh?: MapMesh;
  private cssW = 1;
  private cssH = 1;

  constructor(private canvas: HTMLCanvasElement) {
    const gl = canvas.getContext('webgl2', { antialias: true, alpha: false, powerPreference: 'high-performance' });
    if (!gl) throw new Error('WebGL2 is not available in this browser');
    this.gl = gl;
    this.fill = this.program(VERT, FILL_FRAG, ['uView', 'uSize', 'uRadius', 'uCopies', 'uPalette', 'uPaletteWidth', 'uHover', 'uSelected']);
    this.line = this.program(VERT, LINE_FRAG, ['uView', 'uSize', 'uRadius', 'uCopies', 'uColor']);
    this.fillVao = gl.createVertexArray()!;
    this.lineVao = gl.createVertexArray()!;
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

  private buffer(target: number, data: ArrayBufferView): WebGLBuffer {
    const gl = this.gl;
    const b = gl.createBuffer()!;
    gl.bindBuffer(target, b);
    gl.bufferData(target, data, gl.STATIC_DRAW);
    this.buffers.push(b);
    return b;
  }

  /** Upload a map. The mesh's buffer views point into WASM memory, so they are consumed right here. */
  setMesh(mesh: MapMesh) {
    const gl = this.gl;
    this.mesh = mesh;
    for (const b of this.buffers) gl.deleteBuffer(b);
    this.buffers = [];

    gl.bindVertexArray(this.fillVao);
    this.buffer(gl.ARRAY_BUFFER, mesh.positions());
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    this.buffer(gl.ARRAY_BUFFER, mesh.vertexProvince());
    gl.enableVertexAttribArray(1);
    gl.vertexAttribIPointer(1, 1, gl.UNSIGNED_INT, 0, 0);
    const idx = mesh.indices();
    this.triIndices = idx.length;
    this.buffer(gl.ELEMENT_ARRAY_BUFFER, idx);

    gl.bindVertexArray(this.lineVao);
    this.buffer(gl.ARRAY_BUFFER, mesh.linePositions());
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.disableVertexAttribArray(1);
    gl.vertexAttribI4ui(1, 0, 0, 0, 0);
    const lidx = mesh.lineIndices();
    this.lineIndices = lidx.length;
    this.buffer(gl.ELEMENT_ARRAY_BUFFER, lidx);
    gl.bindVertexArray(null);

    // One palette texel per province, in rows of up to 4096.
    const n = mesh.len;
    this.paletteWidth = Math.min(Math.max(n, 1), 4096);
    const rows = Math.ceil(Math.max(n, 1) / this.paletteWidth);
    const texels = new Uint8Array(this.paletteWidth * rows * 4);
    texels.set(mesh.palette());
    gl.bindTexture(gl.TEXTURE_2D, this.palette);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, this.paletteWidth, rows, 0, gl.RGBA, gl.UNSIGNED_BYTE, texels);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  }

  /** Give provinces new colors: `rgba` holds four bytes per province, from id 0. */
  setPalette(rgba: Uint8Array) {
    const gl = this.gl;
    gl.bindTexture(gl.TEXTURE_2D, this.palette);
    const rows = Math.ceil(rgba.length / 4 / this.paletteWidth);
    const texels = new Uint8Array(this.paletteWidth * rows * 4);
    texels.set(rgba);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, this.paletteWidth, rows, gl.RGBA, gl.UNSIGNED_BYTE, texels);
  }

  resize(cssW: number, cssH: number, dpr: number) {
    this.cssW = Math.max(1, cssW);
    this.cssH = Math.max(1, cssH);
    this.canvas.width = Math.max(1, Math.round(this.cssW * dpr));
    this.canvas.height = Math.max(1, Math.round(this.cssH * dpr));
  }

  draw(cam: Camera, hover: number | null, selected: number | null) {
    const t0 = performance.now();
    const gl = this.gl;
    gl.viewport(0, 0, this.canvas.width, this.canvas.height);
    gl.clearColor(0.078, 0.086, 0.102, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    if (!this.mesh) return;

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
    gl.uniform1ui(this.fill.loc.uHover, hover ?? NONE);
    gl.uniform1ui(this.fill.loc.uSelected, selected ?? NONE);
    gl.bindVertexArray(this.fillVao);
    gl.drawElements(gl.TRIANGLES, this.triIndices, gl.UNSIGNED_INT, 0);

    // Borders, then outlines of the hovered and selected provinces.
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    gl.useProgram(this.line.prog);
    gl.bindVertexArray(this.lineVao);
    setCommon(this.line, 0, 1);
    gl.uniform4f(this.line.loc.uColor, 0, 0, 0, 0.3);
    gl.drawElements(gl.LINES, this.lineIndices, gl.UNSIGNED_INT, 0);

    const outline = (id: number | null, r: number, g: number, b: number, radius: number) => {
      if (id === null) return;
      const [first, count] = this.mesh!.lineRange(id);
      setCommon(this.line, radius, 8);
      gl.uniform4f(this.line.loc.uColor, r, g, b, 1);
      gl.drawElementsInstanced(gl.LINES, count, gl.UNSIGNED_INT, first * 4, 8);
    };
    outline(hover, 1, 1, 1, 1);
    outline(selected, 1, 0.83, 0, 1.5);

    gl.bindVertexArray(null);
    renderStats.draws++;
    renderStats.cpuMs += performance.now() - t0;
  }

  dispose() {
    const gl = this.gl;
    for (const b of this.buffers) gl.deleteBuffer(b);
    gl.deleteTexture(this.palette);
    gl.deleteVertexArray(this.fillVao);
    gl.deleteVertexArray(this.lineVao);
    gl.deleteProgram(this.fill.prog);
    gl.deleteProgram(this.line.prog);
    this.buffers = [];
  }
}
