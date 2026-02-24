#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Clone)]
pub struct RecognitionResult {
    pub text: String,
    pub input_id: String,
    pub timestamp: f64,
    pub is_final: bool,
}

#[derive(Debug, Clone)]
pub struct TextMetadata {
    pub input_id: String,
    pub prefix: String,
}
