const GRID_DIAMETER: f32 = 146.25;

#[must_use]
pub fn get_grid_pos(x: f32, y: f32, map_size: u32) -> String {
    #[allow(clippy::cast_precision_loss)]
    let map_size_f = map_size as f32;
    let corrected_map_size = get_corrected_map_size(map_size_f);

    if x < 0.0 || x > corrected_map_size || y < 0.0 || y > corrected_map_size {
        return "Outside Grid".to_string();
    }

    let grid_pos_letters = get_grid_pos_letters_x(x);
    let grid_pos_number = get_grid_pos_number_y(y, corrected_map_size);

    format!("{grid_pos_letters}{grid_pos_number}")
}

fn get_grid_pos_letters_x(x: f32) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let counter = (x / GRID_DIAMETER).floor() as u32 + 1;
    number_to_letters(counter)
}

fn get_grid_pos_number_y(y: f32, map_size: f32) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let number_of_grids = (map_size / GRID_DIAMETER).floor() as u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let counter = (y / GRID_DIAMETER).floor() as u32 + 1;
    number_of_grids.saturating_sub(counter)
}

fn number_to_letters(mut num: u32) -> String {
    let mut letters = String::new();
    while num > 0 {
        let mod_val = (num - 1) % 26;
        letters.insert(0, (b'A' + mod_val as u8) as char);
        num = (num - mod_val) / 26;
    }
    letters
}

fn get_corrected_map_size(map_size: f32) -> f32 {
    let remainder = map_size % GRID_DIAMETER;
    if remainder < 120.0 {
        map_size - remainder
    } else {
        map_size + (GRID_DIAMETER - remainder)
    }
}
