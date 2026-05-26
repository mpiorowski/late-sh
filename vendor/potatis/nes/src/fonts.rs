use phf::phf_map;

use crate::frame::RenderFrame;

const W: usize = 5;

static FONTS: phf::Map<char, &'static str> = phf_map! {
 '0' => r#"
    ....
    .  .
    .  .
    .  .
    .  .
    .  .
    ...."#,

 '1' => r#"
     ..
      .
      .
      .
      .
      .
      ."#,

 '2' => r#"
    ...
      .
      .
      .
    ... 
    .   
    ...."#,

 '3' => r#"
    ...
      .
      .
    ...
      .
      .
    ..."#,

 '4' => r#"
    .  .
    .  .
    .  .
    ....
       .
       .
       ."#,

 '5' => r#"
    ....
    .  
    .  
    ....
       .
       .
    ...."#,

 '6' => r#"
    ....
    .  
    .  
    ....
    .  .
    .  .
    ...."#,

 '7' => r#"
    ....
       .
       .
      .  
      . 
     .
     .  "#,

 '8' => r#"
    ....
    .  .
    .  .
    ....
    .  .
    .  .
    ...."#,

 '9' => r#"
    ....
    .  .
    .  .
    ....
       .
       .
       ."#
};

pub fn draw(s: &str, pos: (usize, usize), frame: &mut RenderFrame) {
  let fonts = s.chars().map(|c| FONTS.get(&c).expect("font missing"));

  for (char_n, font) in fonts.enumerate() {
    let char_base_x = pos.0 + char_n * W;
    let mut x = char_base_x;
    let mut y = pos.1;

    font.chars().for_each(|c| match c {
      '\n' => {
        y += 1;
        x = char_base_x;
      }
      '.' => {
        frame.set_pixel_xy(x, y, (0xff, 0, 0));
        x += 1;
      }
      ' ' => x += 1,
      _ => (),
    });
  }
}
