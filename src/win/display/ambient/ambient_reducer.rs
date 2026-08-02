struct AmbientReducer {
    scores: [f32; BIN_COUNT],
    sum_r: [f32; BIN_COUNT],
    sum_g: [f32; BIN_COUNT],
    sum_b: [f32; BIN_COUNT],
    ignored_black: u32,
}

impl AmbientReducer {
    fn new() -> Self {
        Self {
            scores: [0.0; BIN_COUNT],
            sum_r: [0.0; BIN_COUNT],
            sum_g: [0.0; BIN_COUNT],
            sum_b: [0.0; BIN_COUNT],
            ignored_black: 0,
        }
    }

    fn clear(&mut self) {
        self.scores.fill(0.0);
        self.sum_r.fill(0.0);
        self.sum_g.fill(0.0);
        self.sum_b.fill(0.0);
        self.ignored_black = 0;
    }

    fn add(&mut self, rgb: Rgb) {
        if is_ignored_black(rgb) {
            self.ignored_black += 1;
            return;
        }

        let weight = ambient_weight(rgb);
        if weight <= 0.0 {
            return;
        }

        let bin = rgb_bin(rgb);
        self.scores[bin] += weight;
        self.sum_r[bin] += rgb.r as f32 * weight;
        self.sum_g[bin] += rgb.g as f32 * weight;
        self.sum_b[bin] += rgb.b as f32 * weight;
    }

    fn finish(&self, sampled_total: u32) -> AmbientColor {
        let considered_score: f32 = self.scores.iter().sum();
        let ignored_black = if sampled_total == 0 {
            0.0
        } else {
            self.ignored_black as f32 / sampled_total as f32
        };
        if considered_score <= f32::EPSILON {
            return AmbientColor {
                rgb: BLACK_FALLBACK,
                bin_rgb: bin_center_rgb(rgb_bin(BLACK_FALLBACK)),
                dominance: 1.0,
                ignored_black: 1.0,
            };
        }

        let (best_bin, best_score) = self
            .scores
            .iter()
            .copied()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .unwrap_or((0, 0.0));
        let top = self.bin_average(best_bin);
        let scene = self.weighted_scene_average().unwrap_or(top);
        let palette = self.salient_palette_average().unwrap_or(top);
        let rgb = boost_red_saturation(
            boost_saturation(mix_rgb(palette, scene, 0.14), FINAL_SATURATION_BOOST),
            RED_SATURATION_BOOST,
        );

        AmbientColor {
            rgb,
            bin_rgb: bin_center_rgb(rgb_bin(rgb)),
            dominance: (best_score / considered_score).clamp(0.0, 1.0),
            ignored_black,
        }
    }

    fn bin_average(&self, bin: usize) -> Rgb {
        let score = self.scores[bin].max(f32::EPSILON);
        Rgb {
            r: (self.sum_r[bin] / score).round().clamp(0.0, 255.0) as u8,
            g: (self.sum_g[bin] / score).round().clamp(0.0, 255.0) as u8,
            b: (self.sum_b[bin] / score).round().clamp(0.0, 255.0) as u8,
        }
    }

    fn salient_palette_average(&self) -> Option<Rgb> {
        let max_score = self.scores.iter().copied().fold(0.0f32, f32::max);
        if max_score <= f32::EPSILON {
            return None;
        }

        let mut total = 0.0;
        let mut r = 0.0;
        let mut g = 0.0;
        let mut b = 0.0;

        for (bin, score) in self.scores.iter().copied().enumerate() {
            if score < max_score * 0.02 {
                continue;
            }

            let weight = score.powf(0.18);
            let avg = self.bin_average(bin);
            total += weight;
            r += avg.r as f32 * weight;
            g += avg.g as f32 * weight;
            b += avg.b as f32 * weight;
        }

        if total <= f32::EPSILON {
            None
        } else {
            Some(Rgb {
                r: (r / total).round().clamp(0.0, 255.0) as u8,
                g: (g / total).round().clamp(0.0, 255.0) as u8,
                b: (b / total).round().clamp(0.0, 255.0) as u8,
            })
        }
    }

    fn weighted_scene_average(&self) -> Option<Rgb> {
        let total: f32 = self.scores.iter().sum();
        if total <= f32::EPSILON {
            return None;
        }

        Some(Rgb {
            r: (self.sum_r.iter().sum::<f32>() / total)
                .round()
                .clamp(0.0, 255.0) as u8,
            g: (self.sum_g.iter().sum::<f32>() / total)
                .round()
                .clamp(0.0, 255.0) as u8,
            b: (self.sum_b.iter().sum::<f32>() / total)
                .round()
                .clamp(0.0, 255.0) as u8,
        })
    }
}

