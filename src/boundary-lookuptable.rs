fn main() {
    for_shape(100, 80, "ACTIVITY", false, "u8");
    for_shape(36, 36, "EVENT", false, "u8");
    for_shape(50, 50, "GATEWAY", false, "u8");
    for_shape(50, 50, "DATASTORE", false, "u8");
    for_shape(36, 50, "DATAOBJECT", false, "u8");
    println!(
        "/// To ensure that the ends of the rays don't intersect with the connected nodes themselves, apply an additional offset for testing purposes."
    );
    for_shape(2, 2, "OFFSET", true, "i8");
}

// AI generated, after I presented AI a version of my own which was rather worse.
// 0° right, clockwise
fn for_shape(max_x: usize, max_y: usize, name: &str, center: bool, datatype: &str) {
    println!("pub const {name}: [({datatype}, {datatype}); 360] = [");
    let offset_x = if center { (max_x / 2) as isize } else { 0 };
    let offset_y = if center { (max_y / 2) as isize } else { 0 };

    let max_x = max_x as f64;
    let max_y = max_y as f64;

    let center_x = max_x / 2.0;
    let center_y = max_y / 2.0;

    for deg in 0..360 {
        let radians = (deg as f64).to_radians();

        let dx = radians.cos();
        let dy = radians.sin();

        let distance_to_vertical_edge = if dx > 0.0 {
            (max_x - center_x) / dx
        } else if dx < 0.0 {
            (0.0 - center_x) / dx
        } else {
            f64::INFINITY
        };

        let distance_to_horizontal_edge = if dy > 0.0 {
            (max_y - center_y) / dy
        } else if dy < 0.0 {
            (0.0 - center_y) / dy
        } else {
            f64::INFINITY
        };

        let distance = distance_to_vertical_edge.min(distance_to_horizontal_edge);

        let x = (center_x + dx * distance).round().clamp(0.0, max_x) as isize - offset_x;

        let y = (center_y + dy * distance).round().clamp(0.0, max_y) as isize - offset_y;

        println!("({x}, {y}),");
    }

    println!("];");
}
