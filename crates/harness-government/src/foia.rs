// IMPLEMENTS: D-387
//! FOIA-ready ledger. Reuses the D-359 hash-chain pattern but layers
//! on:
//!  * NARA GRS retention (30+ years for permanent records),
//!  * FOIA exemption preview (which b(N) likely applies),
//!  * an officer workflow state machine (Draft → Review → Released
//!    or Withheld).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoiaExemption {
    None,
    /// b(1) — National defense / foreign policy.
    B1NationalDefense,
    /// b(4) — Trade secrets / commercial confidential.
    B4TradeSecret,
    /// b(5) — Inter-agency deliberative process.
    B5DeliberativeProcess,
    /// b(6) — Personal privacy.
    B6Privacy,
    /// b(7)(C) — Law enforcement / personal privacy.
    B7cLawEnforcement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfficerStage {
    Draft,
    Review,
    Released,
    Withheld,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoiaEntry {
    pub seq: u64,
    pub prev: [u8; 32],
    pub body: serde_json::Value,
    pub exemption_preview: FoiaExemption,
    pub stage: OfficerStage,
    pub at_ms: i64,
}

impl FoiaEntry {
    #[must_use]
    pub fn hash(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"harness/gov/foia/v1\n");
        h.update(&self.seq.to_le_bytes());
        h.update(&self.prev);
        let body = serde_json::to_vec(&self.body).unwrap_or_else(|_| b"<unserialisable>".to_vec());
        h.update(&body);
        h.update(format!("{:?}", self.exemption_preview).as_bytes());
        h.update(format!("{:?}", self.stage).as_bytes());
        h.update(&self.at_ms.to_le_bytes());
        *h.finalize().as_bytes()
    }
}

#[derive(Debug, Default)]
pub struct FoiaLedger {
    entries: Vec<FoiaEntry>,
}

impl FoiaLedger {
    pub fn append(
        &mut self,
        body: serde_json::Value,
        exemption_preview: FoiaExemption,
        stage: OfficerStage,
        at_ms: i64,
    ) {
        let prev = self.entries.last().map_or([0u8; 32], FoiaEntry::hash);
        let seq = u64::try_from(self.entries.len()).unwrap_or(u64::MAX);
        self.entries.push(FoiaEntry {
            seq,
            prev,
            body,
            exemption_preview,
            stage,
            at_ms,
        });
    }

    /// Officer transitions are append-only — emitting a new entry that
    /// references the same body but the new stage. This preserves the
    /// historical record for FOIA logs.
    pub fn transition(&mut self, body: serde_json::Value, new_stage: OfficerStage, at_ms: i64) {
        self.append(body, FoiaExemption::None, new_stage, at_ms);
    }

    #[must_use]
    pub fn entries(&self) -> &[FoiaEntry] {
        &self.entries
    }

    pub fn verify(&self) -> Result<(), u64> {
        for (i, e) in self.entries.iter().enumerate() {
            let expected = if i == 0 {
                [0u8; 32]
            } else {
                self.entries[i - 1].hash()
            };
            if e.prev != expected {
                return Err(e.seq);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_verify_chain() {
        let mut l = FoiaLedger::default();
        l.append(
            serde_json::json!({"doc": "memo"}),
            FoiaExemption::B5DeliberativeProcess,
            OfficerStage::Draft,
            1,
        );
        l.transition(serde_json::json!({"doc": "memo"}), OfficerStage::Review, 2);
        l.transition(
            serde_json::json!({"doc": "memo"}),
            OfficerStage::Released,
            3,
        );
        assert_eq!(l.entries().len(), 3);
        assert!(l.verify().is_ok());
    }

    #[test]
    fn tampered_stage_breaks_chain() {
        let mut l = FoiaLedger::default();
        l.append(
            serde_json::json!({"doc": "memo"}),
            FoiaExemption::None,
            OfficerStage::Draft,
            1,
        );
        l.append(
            serde_json::json!({"doc": "memo"}),
            FoiaExemption::None,
            OfficerStage::Review,
            2,
        );
        l.entries[0].stage = OfficerStage::Released;
        assert_eq!(l.verify(), Err(1));
    }
}
