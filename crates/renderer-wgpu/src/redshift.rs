//! Relativistic Doppler-shift color reconstruction for display-referred RGB images.
//!
//! An RGB pixel is a tristimulus value, not a measured spectrum.  We therefore
//! reconstruct it from three smooth spectral basis functions, calibrate that
//! reconstruction to reproduce linear sRGB exactly at rest, shift the basis in
//! wavelength, and integrate it against analytic CIE 1931 matching functions.

use std::sync::OnceLock;

pub const MAX_DOPPLER_BETA: f32 = 0.9999;

pub type ColorMatrix = [[f32; 3]; 3];

const IDENTITY: ColorMatrix = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
const VISIBLE_MIN_NM: f64 = 380.0;
const VISIBLE_MAX_NM: f64 = 780.0;
const INTEGRATION_STEP_NM: f64 = 2.0;

/// Returns the longitudinal relativistic wavelength multiplier at `beta * c`.
/// Positive velocity means separation/recession; negative velocity means
/// approach.
#[must_use]
pub fn relativistic_doppler_factor(beta: f32) -> f32 {
    let beta = clamped_beta(beta);
    ((1.0 + beta) / (1.0 - beta)).sqrt()
}

/// Builds a linear-sRGB matrix that previews a relativistic Doppler shift.
///
/// The wavelength mapping is exact for longitudinal recession. Spectral
/// reconstruction is necessarily approximate because an RGB image contains
/// only three color measurements and no ultraviolet or infrared information.
#[must_use]
pub fn doppler_shift_color_matrix(beta: f32) -> ColorMatrix {
    let beta = clamped_beta(beta);
    if beta.abs() <= f32::EPSILON {
        return IDENTITY;
    }

    let source_inverse = *SOURCE_BASIS_INVERSE.get_or_init(|| {
        invert(basis_to_linear_srgb(1.0)).expect("spectral basis must be independent")
    });
    multiply(
        basis_to_linear_srgb(f64::from(relativistic_doppler_factor(beta))),
        source_inverse,
    )
}

fn clamped_beta(beta: f32) -> f32 {
    if beta.is_finite() {
        beta.clamp(-MAX_DOPPLER_BETA, MAX_DOPPLER_BETA)
    } else {
        0.0
    }
}

static SOURCE_BASIS_INVERSE: OnceLock<ColorMatrix> = OnceLock::new();

fn basis_to_linear_srgb(doppler_factor: f64) -> ColorMatrix {
    let mut xyz_columns = [[0.0_f64; 3]; 3];
    let sample_count = ((VISIBLE_MAX_NM - VISIBLE_MIN_NM) / INTEGRATION_STEP_NM) as usize;

    for sample in 0..=sample_count {
        let observed_wavelength = VISIBLE_MIN_NM + sample as f64 * INTEGRATION_STEP_NM;
        let source_wavelength = observed_wavelength / doppler_factor;
        if !(VISIBLE_MIN_NM..=VISIBLE_MAX_NM).contains(&source_wavelength) {
            continue;
        }

        let matching = cie_1931_matching(observed_wavelength);
        let basis = spectral_basis(source_wavelength);
        // The 1/D Jacobian preserves integrated spectral energy while moving
        // wavelength bins. Absolute relativistic radiance cannot be recovered
        // from an exposure-normalized photograph.
        let weight = INTEGRATION_STEP_NM / doppler_factor;
        for column in 0..3 {
            for component in 0..3 {
                xyz_columns[column][component] += matching[component] * basis[column] * weight;
            }
        }
    }

    let mut matrix = [[0.0_f32; 3]; 3];
    for column in 0..3 {
        let rgb = xyz_to_linear_srgb(xyz_columns[column]);
        for row in 0..3 {
            matrix[row][column] = rgb[row] as f32;
        }
    }
    matrix
}

fn spectral_basis(wavelength_nm: f64) -> [f64; 3] {
    [
        gaussian(wavelength_nm, 611.0, 32.0),
        gaussian(wavelength_nm, 549.0, 26.0),
        gaussian(wavelength_nm, 464.0, 20.0),
    ]
}

fn gaussian(value: f64, center: f64, width: f64) -> f64 {
    (-0.5 * ((value - center) / width).powi(2)).exp()
}

// Analytic fits to the CIE 1931 2-degree color matching functions from
// Wyman, Sloan, and Shirley, "Simple Analytic Approximations to the CIE XYZ
// Color Matching Functions" (2013).
fn cie_1931_matching(wavelength_nm: f64) -> [f64; 3] {
    let x = 0.362 * asymmetric_gaussian(wavelength_nm, 442.0, 0.0624, 0.0374)
        + 1.056 * asymmetric_gaussian(wavelength_nm, 599.8, 0.0264, 0.0323)
        - 0.065 * asymmetric_gaussian(wavelength_nm, 501.1, 0.0490, 0.0382);
    let y = 0.821 * asymmetric_gaussian(wavelength_nm, 568.8, 0.0213, 0.0247)
        + 0.286 * asymmetric_gaussian(wavelength_nm, 530.9, 0.0613, 0.0322);
    let z = 1.217 * asymmetric_gaussian(wavelength_nm, 437.0, 0.0845, 0.0278)
        + 0.681 * asymmetric_gaussian(wavelength_nm, 459.0, 0.0385, 0.0725);
    [x, y, z]
}

fn asymmetric_gaussian(wavelength_nm: f64, center: f64, left_scale: f64, right_scale: f64) -> f64 {
    let scale = if wavelength_nm < center {
        left_scale
    } else {
        right_scale
    };
    (-0.5 * ((wavelength_nm - center) * scale).powi(2)).exp()
}

fn xyz_to_linear_srgb(xyz: [f64; 3]) -> [f64; 3] {
    [
        3.240_454_2 * xyz[0] - 1.537_138_5 * xyz[1] - 0.498_531_4 * xyz[2],
        -0.969_266 * xyz[0] + 1.876_010_8 * xyz[1] + 0.041_556 * xyz[2],
        0.055_643_4 * xyz[0] - 0.204_025_9 * xyz[1] + 1.057_225_2 * xyz[2],
    ]
}

fn multiply(left: ColorMatrix, right: ColorMatrix) -> ColorMatrix {
    let mut result = [[0.0; 3]; 3];
    for row in 0..3 {
        for column in 0..3 {
            result[row][column] = (0..3)
                .map(|inner| left[row][inner] * right[inner][column])
                .sum();
        }
    }
    result
}

fn invert(matrix: ColorMatrix) -> Option<ColorMatrix> {
    let determinant = matrix[0][0] * (matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1])
        - matrix[0][1] * (matrix[1][0] * matrix[2][2] - matrix[1][2] * matrix[2][0])
        + matrix[0][2] * (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0]);
    if determinant.abs() <= f32::EPSILON {
        return None;
    }

    let inverse_determinant = determinant.recip();
    Some([
        [
            (matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1]) * inverse_determinant,
            (matrix[0][2] * matrix[2][1] - matrix[0][1] * matrix[2][2]) * inverse_determinant,
            (matrix[0][1] * matrix[1][2] - matrix[0][2] * matrix[1][1]) * inverse_determinant,
        ],
        [
            (matrix[1][2] * matrix[2][0] - matrix[1][0] * matrix[2][2]) * inverse_determinant,
            (matrix[0][0] * matrix[2][2] - matrix[0][2] * matrix[2][0]) * inverse_determinant,
            (matrix[0][2] * matrix[1][0] - matrix[0][0] * matrix[1][2]) * inverse_determinant,
        ],
        [
            (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0]) * inverse_determinant,
            (matrix[0][1] * matrix[2][0] - matrix[0][0] * matrix[2][1]) * inverse_determinant,
            (matrix[0][0] * matrix[1][1] - matrix[0][1] * matrix[1][0]) * inverse_determinant,
        ],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relativistic_factor_matches_known_values() {
        assert!((relativistic_doppler_factor(0.0) - 1.0).abs() < f32::EPSILON);
        assert!((relativistic_doppler_factor(0.6) - 2.0).abs() < 1.0e-6);
        assert!((relativistic_doppler_factor(-0.6) - 0.5).abs() < 1.0e-6);
    }

    #[test]
    fn zero_speed_is_an_exact_identity() {
        assert_eq!(doppler_shift_color_matrix(0.0), IDENTITY);
    }

    #[test]
    fn visible_only_input_has_shifted_out_of_view_by_point_seven_c_both_ways() {
        for beta in [-0.7, 0.7] {
            let matrix = doppler_shift_color_matrix(beta);
            assert!(matrix.iter().flatten().all(|value| value.abs() < 1.0e-6));
        }
    }

    #[test]
    fn receding_blue_light_moves_toward_red() {
        let matrix = doppler_shift_color_matrix(0.28);
        let shifted_blue = [matrix[0][2], matrix[1][2], matrix[2][2]];
        assert!(shifted_blue[0] > shifted_blue[2]);
        assert!(shifted_blue[0] > shifted_blue[1]);
    }

    #[test]
    fn approaching_red_light_moves_toward_blue() {
        let matrix = doppler_shift_color_matrix(-0.28);
        let shifted_red = [matrix[0][0], matrix[1][0], matrix[2][0]];
        assert!(shifted_red[2] > shifted_red[0]);
        assert!(shifted_red[2] > shifted_red[1]);
    }
}
