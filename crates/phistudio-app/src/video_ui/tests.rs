use super::*;
#[test]
fn rgba_rows_and_unicode_names_are_preserved() {
    let mut data = (0..24).collect::<Vec<u8>>();
    flip_rows(&mut data, 2, 3);
    assert_eq!(
        data,
        [
            16, 17, 18, 19, 20, 21, 22, 23, 8, 9, 10, 11, 12, 13, 14, 15, 0, 1, 2, 3, 4, 5, 6, 7
        ]
    );
    assert_eq!(movie_name(" ../测试:曲目.mp4 "), "..测试曲目.mp4");
}
