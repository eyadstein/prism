//! Optical glass catalogue using Sellmeier dispersion equations.
//!
//! `n^2(l) = 1 + sum_i B_i l^2 / (l^2 - C_i)` with `l` in micrometres.

const D_LINE_NM: f64 = 587.56;
const F_LINE_NM: f64 = 486.13;
const C_LINE_NM: f64 = 656.27;
const G_LINE_NM: f64 = 435.83;

/// A dispersive transparent material.
#[derive(Debug)]
pub struct Glass {
    /// Catalogue name.
    pub name: &'static str,
    b: &'static [f64],
    c: &'static [f64],
}

impl Glass {
    /// Refractive index at `nm` nanometres.
    pub fn index(&self, nm: f64) -> f64 {
        let l2 = (nm / 1000.0) * (nm / 1000.0);
        let s: f64 = self
            .b
            .iter()
            .zip(self.c)
            .map(|(b, c)| b * l2 / (l2 - c))
            .sum();
        (1.0 + s).sqrt()
    }

    /// Abbe number `(nd - 1) / (nF - nC)`; lower means stronger dispersion.
    pub fn abbe(&self) -> f64 {
        (self.index(D_LINE_NM) - 1.0) / (self.index(F_LINE_NM) - self.index(C_LINE_NM))
    }

    /// Relative partial dispersion `(nG - nF) / (nF - nC)` between the g, F and C lines.
    /// Most glasses lie close to one straight line when this is plotted against the Abbe
    /// number. Glasses far off that line (fluorite crowns) make apochromatic lenses possible.
    pub fn partial_dispersion(&self) -> f64 {
        (self.index(G_LINE_NM) - self.index(F_LINE_NM))
            / (self.index(F_LINE_NM) - self.index(C_LINE_NM))
    }

    /// Looks a glass up by case-insensitive name.
    pub fn by_name(name: &str) -> Option<&'static Self> {
        catalog()
            .iter()
            .copied()
            .find(|g| g.name.eq_ignore_ascii_case(name))
    }
}

/// Schott N-BK7 borosilicate crown glass.
pub static BK7: Glass = Glass {
    name: "N-BK7",
    b: &[1.03961212, 0.231792344, 1.01046945],
    c: &[0.00600069867, 0.0200179144, 103.560653],
};

/// Fused silica.
pub static FUSED_SILICA: Glass = Glass {
    name: "FUSED-SILICA",
    b: &[0.6961663, 0.4079426, 0.8974794],
    c: &[0.00467914826, 0.0135120631, 97.9340025],
};

/// Schott F2 flint glass.
pub static F2: Glass = Glass {
    name: "F2",
    b: &[1.34533359, 0.209073176, 0.937357162],
    c: &[0.00997743871, 0.0470450767, 111.886764],
};

/// Schott SF11 dense flint glass (strong dispersion).
pub static SF11: Glass = Glass {
    name: "SF11",
    b: &[1.73759695, 0.313747346, 1.89878101],
    c: &[0.013188707, 0.0623068142, 155.23629],
};

/// Sapphire, ordinary ray.
pub static SAPPHIRE: Glass = Glass {
    name: "SAPPHIRE",
    b: &[1.4313493, 0.65054713, 5.3414021],
    c: &[0.0052799261, 0.0142382647, 325.01783],
};

/// Diamond.
pub static DIAMOND: Glass = Glass {
    name: "DIAMOND",
    b: &[0.3306, 4.3356],
    c: &[0.030625, 0.011236],
};

/// Water at room temperature.
pub static WATER: Glass = Glass {
    name: "WATER",
    b: &[0.5684027565, 0.1726177391, 0.02086189578, 0.1130748688],
    c: &[0.005101829712, 0.01821153936, 0.02620722293, 10.69792721],
};

/// Schott N-FK51A fluorite crown glass: very low dispersion and anomalous partial dispersion.
pub static N_FK51A: Glass = Glass {
    name: "N-FK51A",
    b: &[0.971247817, 0.216901417, 0.904651666],
    c: &[0.00472301995, 0.0153575612, 168.68133],
};

/// Calcium fluoride (fluorite): a crystal with extremely low dispersion.
pub static CAF2: Glass = Glass {
    name: "CAF2",
    b: &[0.5675888, 0.4710914, 3.8484723],
    c: &[0.00252643, 0.01007833, 1200.556],
};

/// Schott N-SF6 dense flint glass.
pub static N_SF6: Glass = Glass {
    name: "N-SF6",
    b: &[1.77931763, 0.338149866, 2.08734474],
    c: &[0.0133714182, 0.0617533621, 174.01759],
};

/// Schott SF2 flint glass.
pub static SF2: Glass = Glass {
    name: "SF2",
    b: &[1.40301821, 0.231767504, 0.939056586],
    c: &[0.0105795466, 0.0493226978, 112.405955],
};

/// Schott N-SK16 dense crown glass.
pub static N_SK16: Glass = Glass {
    name: "N-SK16",
    b: &[1.34317774, 0.241144399, 0.994317969],
    c: &[0.00704687339, 0.0229005, 92.7508526],
};

static CATALOG: [&Glass; 12] = [
    &BK7,
    &FUSED_SILICA,
    &F2,
    &SF11,
    &SAPPHIRE,
    &DIAMOND,
    &WATER,
    &N_FK51A,
    &CAF2,
    &N_SF6,
    &SF2,
    &N_SK16,
];

/// Every built-in material.
pub fn catalog() -> &'static [&'static Glass] {
    &CATALOG
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d_line_indices_match_datasheets() {
        let table = [
            (&BK7, 1.5168, 1e-3),
            (&FUSED_SILICA, 1.4585, 2e-3),
            (&F2, 1.6200, 3e-3),
            (&SF11, 1.7847, 3e-3),
            (&SAPPHIRE, 1.7682, 3e-3),
            (&DIAMOND, 2.4175, 5e-3),
            (&WATER, 1.3330, 2e-3),
            (&N_FK51A, 1.4866, 2e-3),
            (&CAF2, 1.4338, 2e-3),
            (&N_SF6, 1.8052, 2e-3),
            (&SF2, 1.6477, 2e-3),
            (&N_SK16, 1.6204, 2e-3),
        ];
        for (glass, expected, tol) in table {
            let n = glass.index(D_LINE_NM);
            assert!((n - expected).abs() < tol, "{} gave {n}", glass.name);
        }
    }

    #[test]
    fn blue_bends_more_than_red() {
        for g in catalog() {
            assert!(g.index(450.0) > g.index(650.0), "{}", g.name);
        }
    }

    #[test]
    fn abbe_numbers() {
        assert!((BK7.abbe() - 64.17).abs() < 0.5);
        assert!((SF11.abbe() - 25.68).abs() < 0.6);
    }

    #[test]
    fn new_glasses_have_sensible_abbe_numbers() {
        assert!((80.0..90.0).contains(&N_FK51A.abbe()), "{}", N_FK51A.abbe());
        assert!((90.0..100.0).contains(&CAF2.abbe()), "{}", CAF2.abbe());
        assert!((22.0..30.0).contains(&N_SF6.abbe()), "{}", N_SF6.abbe());
        assert!((30.0..38.0).contains(&SF2.abbe()), "{}", SF2.abbe());
        assert!((57.0..64.0).contains(&N_SK16.abbe()), "{}", N_SK16.abbe());
    }

    #[test]
    fn partial_dispersion_matches_the_datasheet() {
        assert!((BK7.partial_dispersion() - 0.5349).abs() < 0.005);
    }

    #[test]
    fn flints_have_larger_partial_dispersion_than_crowns() {
        let crown = BK7.partial_dispersion();
        assert!(F2.partial_dispersion() > crown + 0.03);
        assert!(SF11.partial_dispersion() > crown + 0.03);
    }

    #[test]
    fn lookup_by_name() {
        assert_eq!(Glass::by_name("n-bk7").map(|g| g.name), Some("N-BK7"));
        assert_eq!(Glass::by_name("caf2").map(|g| g.name), Some("CAF2"));
        assert!(Glass::by_name("unobtainium").is_none());
    }

    #[test]
    fn catalogue_names_are_unique() {
        let mut names: Vec<&str> = catalog().iter().map(|g| g.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), catalog().len());
        assert_eq!(catalog().len(), 12);
    }
}
