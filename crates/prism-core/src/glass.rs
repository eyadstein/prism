//! Optical glass catalogue using Sellmeier dispersion equations.
//!
//! `n^2(l) = 1 + sum_i B_i l^2 / (l^2 - C_i)` with `l` in micrometres.

const D_LINE_NM: f64 = 587.56;
const F_LINE_NM: f64 = 486.13;
const C_LINE_NM: f64 = 656.27;

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

    /// Looks a glass up by case-insensitive name.
    pub fn by_name(name: &str) -> Option<&'static Self> {
        catalog()
            .into_iter()
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

/// Every built-in material.
pub fn catalog() -> [&'static Glass; 7] {
    [&BK7, &FUSED_SILICA, &F2, &SF11, &SAPPHIRE, &DIAMOND, &WATER]
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
    fn lookup_by_name() {
        assert_eq!(Glass::by_name("n-bk7").map(|g| g.name), Some("N-BK7"));
        assert!(Glass::by_name("unobtainium").is_none());
    }
}
