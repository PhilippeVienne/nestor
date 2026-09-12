//! Voice Activity Detection via Silero VAD v5 (ONNX, execution CUDA).
//!
//! Le modele combine v5 attend, pour chaque fenetre de 512 echantillons
//! (16 kHz), un prefixe de 64 echantillons de "contexte" correspondant a la
//! toute fin de la fenetre precedente (zeros au tout debut). Sans ce prefixe,
//! le modele reste quasiment insensible a la parole (sortie proche de 0 quel
//! que soit le contenu audio) - comportement verifie empiriquement et
//! confirme par l'implementation de reference (`OnnxWrapper.__call__` dans
//! `silero_vad/utils_vad.py` du depot officiel).

use std::path::Path;

use anyhow::{Context, Result};
use ort::ep::CUDA;
use ort::session::Session;
use ort::value::Tensor;

/// Etat recurrent de Silero VAD v5 : forme [2, 1, 128].
const STATE_LEN: usize = 2 * 128;
/// Nombre d'echantillons de contexte a prefixer a chaque fenetre (16 kHz).
const CONTEXT_LEN: usize = 64;

pub struct SileroVad {
    session: Session,
    state: Vec<f32>,
    context: Vec<f32>,
}

impl SileroVad {
    pub fn load(model_path: &Path) -> Result<Self> {
        let session = Session::builder()
            .map_err(|e| anyhow::anyhow!("creation du builder ONNX Runtime: {e}"))?
            .with_execution_providers([CUDA::default().build()])
            .map_err(|e| anyhow::anyhow!("activation de l'EP CUDA pour Silero VAD: {e}"))?
            .commit_from_file(model_path)
            .with_context(|| format!("chargement du modele {}", model_path.display()))?;

        Ok(Self { session, state: vec![0.0; STATE_LEN], context: vec![0.0; CONTEXT_LEN] })
    }

    /// Analyse une fenetre de 512 echantillons (32 ms a 16 kHz) et retourne
    /// la probabilite de parole [0.0, 1.0].
    pub fn process(&mut self, chunk: &[f32]) -> Result<f32> {
        let mut windowed = Vec::with_capacity(CONTEXT_LEN + chunk.len());
        windowed.extend_from_slice(&self.context);
        windowed.extend_from_slice(chunk);

        let input = Tensor::from_array((vec![1i64, windowed.len() as i64], windowed.clone()))?;
        let state_in = Tensor::from_array((vec![2i64, 1, 128], self.state.clone()))?;
        let sr = Tensor::from_array((Vec::<i64>::new(), vec![16_000i64]))?;

        let outputs = self.session.run(ort::inputs![
            "input" => input,
            "state" => state_in,
            "sr" => sr,
        ])?;

        let (_, prob) = outputs["output"].try_extract_tensor::<f32>()?;
        let (_, new_state) = outputs["stateN"].try_extract_tensor::<f32>()?;
        self.state = new_state.to_vec();
        self.context = windowed[windowed.len() - CONTEXT_LEN..].to_vec();

        Ok(prob.first().copied().unwrap_or(0.0))
    }
}
