#[derive(Debug, Clone)]
pub struct Region {
    pub start_sec: f64,
    pub end_sec: f64,
    pub samples: Vec<f32>,
}
