use maptool_core::{Options, PixelFormat};
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;

/// One province: exact-color region(s) with an SVG path.
#[pyclass(frozen, module = "maptool")]
struct Province {
    #[pyo3(get)]
    id: u32,
    #[pyo3(get)]
    color: (u8, u8, u8),
    #[pyo3(get)]
    pixel_count: u32,
    /// `(x0, y0, x1, y1)` in pixels, upper bound exclusive.
    #[pyo3(get)]
    bbox: (u32, u32, u32, u32),
    /// SVG path data.
    #[pyo3(get)]
    path: String,
}

#[pymethods]
impl Province {
    fn __repr__(&self) -> String {
        let (r, g, b) = self.color;
        format!("Province(id={}, color=#{r:02x}{g:02x}{b:02x}, pixel_count={})", self.id, self.pixel_count)
    }
}

impl From<&maptool_core::Province> for Province {
    fn from(p: &maptool_core::Province) -> Self {
        let [r, g, b] = p.color;
        let [x0, y0, x1, y1] = p.bbox;
        Province {
            id: p.id,
            color: (r, g, b),
            pixel_count: p.pixel_count,
            bbox: (x0, y0, x1, y1),
            path: p.path.clone(),
        }
    }
}

#[pyclass(frozen, module = "maptool")]
struct VectorMap(maptool_core::VectorMap);

#[pymethods]
impl VectorMap {
    #[getter]
    fn width(&self) -> u32 {
        self.0.width
    }

    #[getter]
    fn height(&self) -> u32 {
        self.0.height
    }

    #[getter]
    fn provinces(&self) -> Vec<Province> {
        self.0.provinces.iter().map(Province::from).collect()
    }

    fn __len__(&self) -> usize {
        self.0.provinces.len()
    }

    fn __getitem__(&self, index: isize) -> PyResult<Province> {
        let n = self.0.provinces.len() as isize;
        let i = if index < 0 { index + n } else { index };
        usize::try_from(i)
            .ok()
            .and_then(|i| self.0.provinces.get(i))
            .map(Province::from)
            .ok_or_else(|| PyIndexError::new_err("province index out of range"))
    }

    /// The whole map as a standalone SVG document.
    fn to_svg(&self) -> String {
        self.0.to_svg()
    }

    fn __repr__(&self) -> String {
        format!("VectorMap({}x{}, {} provinces)", self.0.width, self.0.height, self.0.provinces.len())
    }
}

fn options(
    tolerance: f64,
    corner_angle: f64,
    min_chain_len: usize,
    precision: usize,
    corner_run: f64,
    validate: bool,
) -> Options {
    Options { tolerance, corner_angle, min_chain_len, precision, corner_run, validate }
}

/// Vectorize tightly packed pixel bytes (`channels` is 3 for RGB, 4 for RGBA).
#[pyfunction]
#[pyo3(signature = (data, width, height, channels=4, tolerance=1.0, corner_angle=100.0, min_chain_len=6, precision=2, corner_run=4.0, validate=true))]
#[allow(clippy::too_many_arguments)]
fn vectorize(
    py: Python<'_>,
    data: &[u8],
    width: u32,
    height: u32,
    channels: u8,
    tolerance: f64,
    corner_angle: f64,
    min_chain_len: usize,
    precision: usize,
    corner_run: f64,
    validate: bool,
) -> PyResult<VectorMap> {
    let format = match channels {
        3 => PixelFormat::Rgb,
        4 => PixelFormat::Rgba,
        n => return Err(PyValueError::new_err(format!("channels must be 3 or 4, got {n}"))),
    };
    let opts = options(tolerance, corner_angle, min_chain_len, precision, corner_run, validate);
    py.detach(|| maptool_core::vectorize(data, width, height, format, &opts))
        .map(VectorMap)
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Decode a PNG or BMP file and vectorize it.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(signature = (path, tolerance=1.0, corner_angle=100.0, min_chain_len=6, precision=2, corner_run=4.0, validate=true))]
fn vectorize_file(
    py: Python<'_>,
    path: std::path::PathBuf,
    tolerance: f64,
    corner_angle: f64,
    min_chain_len: usize,
    precision: usize,
    corner_run: f64,
    validate: bool,
) -> PyResult<VectorMap> {
    let opts = options(tolerance, corner_angle, min_chain_len, precision, corner_run, validate);
    py.detach(|| maptool_core::vectorize_file(&path, &opts))
        .map(VectorMap)
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Province>()?;
    m.add_class::<VectorMap>()?;
    m.add_function(wrap_pyfunction!(vectorize, m)?)?;
    m.add_function(wrap_pyfunction!(vectorize_file, m)?)?;
    Ok(())
}
