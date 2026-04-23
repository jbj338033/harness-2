// IMPLEMENTS: D-413
//! Multi-jurisdiction tax filing checklist.
//!  * IRS Form 5472 (US — foreign-owned single-member LLC).
//!  * 한국 조특법 17조 (Korea — venture company tax credit).
//!  * EU VAT OSS (European one-stop-shop VAT scheme).
//!  * JP QII (Japan Qualified Invoice Issuer).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxFiling {
    IrsForm5472,
    KrJoTeukBeop17,
    EuVatOss,
    JpQii,
}

#[must_use]
pub fn all_tax_filings() -> &'static [TaxFiling] {
    use TaxFiling::*;
    const ALL: &[TaxFiling] = &[IrsForm5472, KrJoTeukBeop17, EuVatOss, JpQii];
    ALL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_filings_listed() {
        assert_eq!(all_tax_filings().len(), 4);
    }

    #[test]
    fn includes_irs_5472_and_kr() {
        let all = all_tax_filings();
        assert!(all.contains(&TaxFiling::IrsForm5472));
        assert!(all.contains(&TaxFiling::KrJoTeukBeop17));
    }

    #[test]
    fn serialises_as_snake_case() {
        let s = serde_json::to_string(&TaxFiling::JpQii).unwrap();
        assert_eq!(s, "\"jp_qii\"");
    }
}
