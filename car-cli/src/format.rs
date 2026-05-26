use car::ColorSpace;
use car::rendition::Idiom;

pub fn color_space_str(cs: &ColorSpace) -> &'static str {
    match cs {
        ColorSpace::SRGB => "srgb",
        ColorSpace::GrayGamma2_2 => "gray-gamma-2.2",
        ColorSpace::DisplayP3 => "display-p3",
        ColorSpace::ExtendedRangeSRGB => "extended-srgb",
        ColorSpace::ExtendedLinearSRGB => "extended-linear-srgb",
        ColorSpace::ExtendedGray => "extended-gray",
        ColorSpace::SystemSRGB => "system-srgb",
        ColorSpace::Unknown { .. } => "unknown",
    }
}

pub fn idiom_str(idiom: &Idiom) -> &'static str {
    match idiom {
        Idiom::Universal => "universal",
        Idiom::Phone => "phone",
        Idiom::Pad => "pad",
        Idiom::TV => "tv",
        Idiom::Car => "car",
        Idiom::Watch => "watch",
        Idiom::Marketing => "marketing",
        Idiom::Unknown { .. } => "unknown",
    }
}
