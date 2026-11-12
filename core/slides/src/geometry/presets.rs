//! the preset outlines, written in a small text form of drawingml's own
//! shape definitions (ecma-376 part 1, annex d, presetShapeDefinitions):
//!
//! - `av name value`: a handle and its default
//! - `gd name formula`: a guide
//! - `path [nofill] [nostroke]` starts a path, then `M x y`, `L x y`,
//!   `A wR hR start swing`, `Q x1 y1 x y`, `C x1 y1 x2 y2 x y`, `Z`
//! - `text l t r b`: where the text goes (guide names)
//!
//! only the common outlines are here; any other preset draws as its box
//! and is listed by `crate::geometry::is_known_preset` as approximate.

pub fn definition(name: &str) -> Option<&'static str> {
    let text = match name {
        "rect" | "flowChartProcess" | "textBox" => RECT,
        "roundRect" | "flowChartAlternateProcess" => ROUND_RECT,
        "ellipse" | "flowChartConnector" => ELLIPSE,
        "triangle" | "flowChartExtract" => TRIANGLE,
        "rtTriangle" => RIGHT_TRIANGLE,
        "diamond" | "flowChartDecision" => DIAMOND,
        "parallelogram" | "flowChartInputOutput" => PARALLELOGRAM,
        "trapezoid" => TRAPEZOID,
        "pentagon" => PENTAGON,
        "hexagon" => HEXAGON,
        "octagon" => OCTAGON,
        "star5" => STAR5,
        "rightArrow" => RIGHT_ARROW,
        "leftArrow" => LEFT_ARROW,
        "upArrow" => UP_ARROW,
        "downArrow" => DOWN_ARROW,
        "chevron" => CHEVRON,
        "homePlate" => HOME_PLATE,
        "plus" | "flowChartOr" => PLUS,
        "donut" => DONUT,
        "line" | "straightConnector1" => LINE,
        "bentConnector2" => BENT_CONNECTOR_2,
        "bentConnector3" => BENT_CONNECTOR_3,
        "curvedConnector3" => CURVED_CONNECTOR_3,
        _ => return None,
    };
    Some(text)
}

const RECT: &str = "
path
M l t
L r t
L r b
L l b
Z
text l t r b";

const ROUND_RECT: &str = "
av adj 16667
gd a pin 0 adj 50000
gd x1 */ ss a 100000
gd x2 +- r 0 x1
gd y2 +- b 0 x1
gd il */ x1 29289 100000
gd ir +- r 0 il
gd ib +- b 0 il
path
M l x1
A x1 x1 cd2 cd4
L x2 t
A x1 x1 3cd4 cd4
L r y2
A x1 x1 0 cd4
L x1 b
A x1 x1 cd4 cd4
Z
text il il ir ib";

const ELLIPSE: &str = "
gd idx cos wd2 2700000
gd idy sin hd2 2700000
gd il +- hc 0 idx
gd ir +- hc idx 0
gd it +- vc 0 idy
gd ib +- vc idy 0
path
M l vc
A wd2 hd2 cd2 cd4
A wd2 hd2 3cd4 cd4
A wd2 hd2 0 cd4
A wd2 hd2 cd4 cd4
Z
text il it ir ib";

const TRIANGLE: &str = "
av adj 50000
gd a pin 0 adj 100000
gd x1 */ w a 200000
gd x2 */ w a 100000
gd x3 +- x1 wd2 0
path
M l b
L x2 t
L r b
Z
text x1 vc x3 b";

const RIGHT_TRIANGLE: &str = "
gd it */ h 7 12
gd ir */ w 7 12
gd ib */ h 11 12
path
M l b
L l t
L r b
Z
text wd12 it ir ib";

const DIAMOND: &str = "
gd ir */ w 3 4
gd ib */ h 3 4
path
M l vc
L hc t
L r vc
L hc b
Z
text wd4 hd4 ir ib";

const PARALLELOGRAM: &str = "
av adj 25000
gd maxAdj */ 100000 w ss
gd a pin 0 adj maxAdj
gd x2 */ ss a 100000
gd x5 +- r 0 x2
path
M l b
L x2 t
L r t
L x5 b
Z
text x2 t x5 b";

const TRAPEZOID: &str = "
av adj 25000
gd maxAdj */ 50000 w ss
gd a pin 0 adj maxAdj
gd x2 */ ss a 100000
gd x3 +- r 0 x2
path
M l b
L x2 t
L x3 t
L r b
Z
text x2 t x3 b";

const PENTAGON: &str = "
av hf 105146
av vf 110557
gd swd2 */ wd2 hf 100000
gd shd2 */ hd2 vf 100000
gd svc */ vc vf 100000
gd dx1 cos swd2 1080000
gd dx2 cos swd2 18360000
gd dy1 sin shd2 1080000
gd dy2 sin shd2 18360000
gd x1 +- hc 0 dx1
gd x2 +- hc 0 dx2
gd x3 +- hc dx2 0
gd x4 +- hc dx1 0
gd y1 +- svc 0 dy1
gd y2 +- svc 0 dy2
path
M x1 y1
L hc t
L x4 y1
L x3 y2
L x2 y2
Z
text x2 y1 x3 y2";

const HEXAGON: &str = "
av adj 25000
av vf 115470
gd maxAdj */ 50000 w ss
gd a pin 0 adj maxAdj
gd shd2 */ hd2 vf 100000
gd x1 */ ss a 100000
gd x2 +- r 0 x1
gd dy1 sin shd2 3600000
gd y1 +- vc 0 dy1
gd y2 +- vc dy1 0
path
M l vc
L x1 y1
L x2 y1
L r vc
L x2 y2
L x1 y2
Z
text x1 y1 x2 y2";

const OCTAGON: &str = "
av adj 29289
gd a pin 0 adj 50000
gd x1 */ ss a 100000
gd x2 +- r 0 x1
gd y2 +- b 0 x1
gd il */ x1 1 2
gd ir +- r 0 il
gd ib +- b 0 il
path
M l x1
L x1 t
L x2 t
L r x1
L r y2
L x2 b
L x1 b
L l y2
Z
text il il ir ib";

const STAR5: &str = "
av adj 19098
av hf 105146
av vf 110557
gd a pin 0 adj 50000
gd swd2 */ wd2 hf 100000
gd shd2 */ hd2 vf 100000
gd svc */ vc vf 100000
gd dx1 cos swd2 1080000
gd dx2 cos swd2 18360000
gd dy1 sin shd2 1080000
gd dy2 sin shd2 18360000
gd x1 +- hc 0 dx1
gd x2 +- hc 0 dx2
gd x3 +- hc dx2 0
gd x4 +- hc dx1 0
gd y1 +- svc 0 dy1
gd y2 +- svc 0 dy2
gd iwd2 */ swd2 a 50000
gd ihd2 */ shd2 a 50000
gd sdx1 cos iwd2 20520000
gd sdx2 cos iwd2 3240000
gd sdy1 sin ihd2 3240000
gd sdy2 sin ihd2 20520000
gd sx1 +- hc 0 sdx1
gd sx2 +- hc 0 sdx2
gd sx3 +- hc sdx2 0
gd sx4 +- hc sdx1 0
gd sy1 +- svc 0 sdy1
gd sy2 +- svc 0 sdy2
gd sy3 +- svc ihd2 0
path
M x1 y1
L sx2 sy1
L hc t
L sx3 sy1
L x4 y1
L sx4 sy2
L x3 y2
L hc sy3
L x2 y2
L sx1 sy2
Z
text sx1 sy1 sx4 sy3";

const RIGHT_ARROW: &str = "
av adj1 50000
av adj2 50000
gd maxAdj2 */ 100000 w ss
gd a1 pin 0 adj1 100000
gd a2 pin 0 adj2 maxAdj2
gd dx1 */ ss a2 100000
gd x1 +- r 0 dx1
gd dy1 */ h a1 200000
gd y1 +- vc 0 dy1
gd y2 +- vc dy1 0
path
M l y1
L x1 y1
L x1 t
L r vc
L x1 b
L x1 y2
L l y2
Z
text l y1 x1 y2";

const LEFT_ARROW: &str = "
av adj1 50000
av adj2 50000
gd maxAdj2 */ 100000 w ss
gd a1 pin 0 adj1 100000
gd a2 pin 0 adj2 maxAdj2
gd x2 */ ss a2 100000
gd dy1 */ h a1 200000
gd y1 +- vc 0 dy1
gd y2 +- vc dy1 0
path
M l vc
L x2 t
L x2 y1
L r y1
L r y2
L x2 y2
L x2 b
Z
text x2 y1 r y2";

const UP_ARROW: &str = "
av adj1 50000
av adj2 50000
gd maxAdj2 */ 100000 h ss
gd a1 pin 0 adj1 100000
gd a2 pin 0 adj2 maxAdj2
gd y2 */ ss a2 100000
gd dx1 */ w a1 200000
gd x1 +- hc 0 dx1
gd x2 +- hc dx1 0
path
M l y2
L hc t
L r y2
L x2 y2
L x2 b
L x1 b
L x1 y2
Z
text x1 y2 x2 b";

const DOWN_ARROW: &str = "
av adj1 50000
av adj2 50000
gd maxAdj2 */ 100000 h ss
gd a1 pin 0 adj1 100000
gd a2 pin 0 adj2 maxAdj2
gd dy1 */ ss a2 100000
gd y1 +- b 0 dy1
gd dx1 */ w a1 200000
gd x1 +- hc 0 dx1
gd x2 +- hc dx1 0
path
M x1 t
L x2 t
L x2 y1
L r y1
L hc b
L l y1
L x1 y1
Z
text x1 t x2 y1";

const CHEVRON: &str = "
av adj 50000
gd maxAdj */ 100000 w ss
gd a pin 0 adj maxAdj
gd x1 */ ss a 100000
gd x2 +- r 0 x1
path
M l t
L x2 t
L r vc
L x2 b
L l b
L x1 vc
Z
text x1 t x2 b";

const HOME_PLATE: &str = "
av adj 50000
gd maxAdj */ 100000 w ss
gd a pin 0 adj maxAdj
gd dx1 */ ss a 100000
gd x1 +- r 0 dx1
path
M l t
L x1 t
L r vc
L x1 b
L l b
Z
text l t x1 b";

const PLUS: &str = "
av adj 25000
gd a pin 0 adj 50000
gd x1 */ ss a 100000
gd x2 +- r 0 x1
gd y2 +- b 0 x1
path
M l x1
L x1 x1
L x1 t
L x2 t
L x2 x1
L r x1
L r y2
L x2 y2
L x2 b
L x1 b
L x1 y2
L l y2
Z
text l x1 r y2";

const DONUT: &str = "
av adj 25000
gd a pin 0 adj 50000
gd dr */ ss a 100000
gd iwd2 +- wd2 0 dr
gd ihd2 +- hd2 0 dr
path
M l vc
A wd2 hd2 cd2 cd4
A wd2 hd2 3cd4 cd4
A wd2 hd2 0 cd4
A wd2 hd2 cd4 cd4
Z
M dr vc
A iwd2 ihd2 cd2 -5400000
A iwd2 ihd2 cd4 -5400000
A iwd2 ihd2 0 -5400000
A iwd2 ihd2 3cd4 -5400000
Z
text l t r b";

const LINE: &str = "
path nofill
M l t
L r b
text l t r b";

const BENT_CONNECTOR_2: &str = "
path nofill
M l t
L r t
L r b
text l t r b";

const BENT_CONNECTOR_3: &str = "
av adj1 50000
gd x1 */ w adj1 100000
path nofill
M l t
L x1 t
L x1 b
L r b
text l t r b";

const CURVED_CONNECTOR_3: &str = "
av adj1 50000
gd x2 */ w adj1 100000
gd x1 +/ l x2 2
gd x3 +/ r x2 2
gd y3 */ h 3 4
path nofill
M l t
C x1 t x2 hd4 x2 vc
C x2 y3 x3 b r b
text l t r b";
