fn get_svg_options() -> &'static resvg::usvg::Options<'static> {
    SVG_OPTIONS.get_or_init(|| {
        let mut opt = resvg::usvg::Options::default();
        *opt.fontdb_mut() = generate_font_db();
        opt.font_family = "Roboto".to_string();
        opt
    })
}

fn generate_font_db() -> resvg::usvg::fontdb::Database {
    let mut font_db = resvg::usvg::fontdb::Database::new();
    let font_bytes = include_bytes!("../../../assets/Roboto.ttf");
    font_db.load_font_data(font_bytes.to_vec());
    font_db
}

fn generate_text_layer_svg(label: &str, no_icon: bool) -> String {
    let font_family_target = "Roboto";
    let y_pos = if no_icon { 85 } else { 115 };
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="100%" height="100%">
            <text x="{}" y="{}" font-family="{}, sans-serif" font-size="20" font-weight="bold" fill="white" text-anchor="middle">{}</text>
        </svg>"#,
        DESIGN_SIZE,
        DESIGN_SIZE,
        (DESIGN_SIZE / 2.0),
        y_pos,
        font_family_target,
        label
    )
}

fn generate_progress_svg(total_steps: usize, active_steps: usize) -> String {
    if total_steps == 0 {
        return format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="100%" height="100%"></svg>"#,
            DESIGN_SIZE, DESIGN_SIZE
        );
    }

    let padding_x = 15.0;
    let bar_y: f64 = 122.0;
    let block_h = 6.0;
    let available_width = DESIGN_SIZE - (padding_x * 2.0);

    let gap = 1.5;
    let total_gaps_width = gap * (total_steps - 1) as f32;
    let block_w = (available_width - total_gaps_width) / total_steps as f32;

    if block_w <= 0.0 {
        return format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="100%" height="100%"></svg>"#,
            DESIGN_SIZE, DESIGN_SIZE
        );
    }

    let mut svg_string = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="100%" height="100%">"#,
        DESIGN_SIZE, DESIGN_SIZE
    );

    for i in 0..total_steps {
        let x_pos = padding_x + i as f32 * (block_w + gap);
        let active_fill = crate::ui::theme::runtime_theme_color().to_hex_string();
        let fill_color = if i < active_steps {
            active_fill.as_str()
        } else {
            "#FFFFFF"
        };
        let fill_opacity = if i < active_steps { "1.0" } else { "0.2" };

        svg_string.push_str(&format!(
            r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" rx="1" fill="{}" fill-opacity="{}" />"#,
            x_pos, bar_y, block_w, block_h, fill_color, fill_opacity
        ));
    }

    svg_string.push_str("</svg>");
    svg_string
}

fn render_svg_to_bytes(
    width: u32,
    height: u32,
    total_steps: usize,
    active_steps: usize,
    label: &str,
    icon_bytes: Option<Cow<'static, [u8]>>,
) -> Option<Vec<u8>> {
    let opt = get_svg_options();

    let frame_svg = include_str!("../../../assets/frame.svg").replace(
        "#F1C40F",
        &crate::ui::theme::runtime_theme_color().to_hex_string(),
    );
    let bg_tree = match resvg::usvg::Tree::from_data(frame_svg.as_bytes(), opt) {
        Ok(tree) => tree,
        Err(error) => {
            warn!(?error, "Failed to parse OSD frame SVG");
            return None;
        }
    };

    let text_svg = generate_text_layer_svg(label, icon_bytes.is_none());
    let text_tree = match resvg::usvg::Tree::from_data(text_svg.as_bytes(), opt) {
        Ok(tree) => tree,
        Err(error) => {
            warn!(?error, "Failed to parse generated OSD text SVG");
            return None;
        }
    };

    let progress_svg = generate_progress_svg(total_steps, active_steps);
    let progress_tree = match resvg::usvg::Tree::from_data(progress_svg.as_bytes(), opt) {
        Ok(tree) => tree,
        Err(error) => {
            warn!(?error, "Failed to parse generated OSD progress SVG");
            return None;
        }
    };

    let Some(mut pixmap) = tiny_skia::Pixmap::new(width, height) else {
        warn!(width, height, "Failed to allocate OSD pixmap");
        return None;
    };

    let view_scale_x = width as f32 / bg_tree.size().width();
    let view_scale_y = height as f32 / bg_tree.size().height();
    let base_transform = tiny_skia::Transform::from_scale(view_scale_x, view_scale_y);

    resvg::render(&bg_tree, base_transform, &mut pixmap.as_mut());

    if let Some(icon_tree) =
        icon_bytes.and_then(|bytes| resvg::usvg::Tree::from_data(&bytes, opt).ok())
    {
        let bg_width_coords = bg_tree.size().width();
        let icon_scale_x = ICON_TARGET_WIDTH / icon_tree.size().width();
        let icon_scale_y = ICON_TARGET_HEIGHT / icon_tree.size().height();
        let icon_pos_x = (bg_width_coords - ICON_TARGET_WIDTH) / 2.0;
        let icon_pos_y = 32.0;

        let icon_transform = tiny_skia::Transform::from_scale(icon_scale_x, icon_scale_y)
            .post_translate(icon_pos_x, icon_pos_y)
            .post_scale(view_scale_x, view_scale_y);

        resvg::render(&icon_tree, icon_transform, &mut pixmap.as_mut());
    }

    resvg::render(&text_tree, base_transform, &mut pixmap.as_mut());
    resvg::render(&progress_tree, base_transform, &mut pixmap.as_mut());

    let mut bgra_pixels = pixmap.data().to_vec();
    for chunk in bgra_pixels.chunks_exact_mut(4) {
        let r = chunk[0];
        let b = chunk[2];
        chunk[0] = b;
        chunk[2] = r;
    }

    Some(bgra_pixels)
}

