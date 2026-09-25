use crate::feature::{self, Feature};

// model_number_prefix shall conform to https://mysupport.razer.com/app/answers/detail/a_id/5481
#[derive(Debug, Clone)]
pub struct Descriptor {
    pub model_number_prefix: &'static str,
    pub name: &'static str,
    pub pid: u16,
    pub features: &'static [&'static str],
}

impl Descriptor {
    /// Whether this model carries a piece of optional hardware.
    ///
    /// Taking the feature as a type rather than a string means a misspelling is
    /// a compile error instead of a silently absent capability — which, for a
    /// gate, would quietly disable the feature on every model.
    pub fn has_feature<F: Feature + Default>(&self) -> bool {
        let name = F::default().name();
        crate::const_for! { it in self.features => {
            if *it == name {
                return true;
            }
        }}
        false
    }
}

pub const SUPPORTED: &[Descriptor] = &[
    Descriptor {
        model_number_prefix: "RZ09-0483T",
        name: "Razer Blade 16” (2023) Black",
        pid: 0x029f,
        features: &[
            "battery-care",
            "fan",
            "kbd-backlight",
            "lid-logo",
            "lights-always-on",
            "perf",
        ],
    },
    Descriptor {
        model_number_prefix: "RZ09-0482X",
        name: "Razer Blade 14” (2023) Mercury",
        pid: 0x029d,
        features: &[
            "battery-care",
            "fan",
            "kbd-backlight",
            "lights-always-on",
            "perf",
        ],
    },
    Descriptor {
        model_number_prefix: "RZ09-05299",
        name: "Razer Blade 18” (2025)",
        pid: 0x02c7,
        features: &[
            "battery-care",
            "fan",
            "kbd-backlight",
            "lid-logo",
            "lights-always-on",
            "perf",
            // The illuminated vent under the chassis. No other supported model
            // has one, so everything that drives it is gated on this.
            "vapour-chamber",
        ],
    },
];

const _VALIDATE_FEATURES: () = {
    crate::const_for! { device in SUPPORTED => {
        feature::validate_features(device.features);
    }}
};

#[cfg(test)]
mod tests {
    use super::*;

    /// The vapour chamber is the one feature the app gates a control and a key
    /// on, so which models claim it is pinned here rather than left to the
    /// table above being read carefully.
    #[test]
    fn only_the_blade_18_has_a_vapour_chamber() {
        for device in SUPPORTED {
            assert_eq!(
                device.has_feature::<feature::VapourChamber>(),
                device.model_number_prefix == "RZ09-05299",
                "{} claims the wrong vapour chamber support",
                device.name
            );
        }
    }

    #[test]
    fn a_feature_the_model_does_not_list_is_absent() {
        let bare = Descriptor {
            model_number_prefix: "RZ09-0000",
            name: "Unknown Blade",
            pid: 0x0000,
            features: &["fan"],
        };

        assert!(bare.has_feature::<feature::Fan>());
        assert!(!bare.has_feature::<feature::VapourChamber>());
    }
}
