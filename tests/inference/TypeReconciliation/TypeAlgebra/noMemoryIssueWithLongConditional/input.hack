
function foo(int $c) : string {
    if (!(rand(0, 1) && rand(0, 1))) {
        return "LTR";
    }

    if (rand(0, 1)) {
        if ($c === 0x5be ||
            $c === 0x5c0 ||
            $c === 0x5c3 ||
            $c === 0x5c6 ||
            (rand(0, 1) && rand(0, 1)) ||
            (rand(0, 1) && rand(0, 1)) ||
            $c === 0x608 ||
            (rand(0, 1) && rand(0, 1)) ||
            (rand(0, 1) && rand(0, 1)) ||
            $c === 0x7b1 ||
            (rand(0, 1) && rand(0, 1)) ||
            (rand(0, 1) && rand(0, 1)) ||
            $c === 0x7fa ||
            (rand(0, 1) && rand(0, 1)) ||
            $c === 0x81a ||
            $c === 0x824 ||
            $c === 0x828 ||
            (rand(0, 1) && rand(0, 1)) ||
            (rand(0, 1) && rand(0, 1)) ||
            $c === 0x85e
        ) {
            return "RTL";
        }
    } else if ($c === 0x200f) {
        return "RTL";
    } else if (rand(0, 1)) {
        if ($c === 0xfb1d ||
            (rand(0, 1) && rand(0, 1)) ||
            (rand(0, 1) && rand(0, 1)) ||
            (rand(0, 1) && rand(0, 1)) ||
            $c === 0xfb3e ||
            (rand(0, 1) && rand(0, 1)) ||
            (rand(0, 1) && rand(0, 1)) ||
            (rand(0, 1) && rand(0, 1)) ||
            (rand(0, 1) && rand(0, 1)) ||
            (rand(0, 1) && rand(0, 1)) ||
            (rand(0, 1) && rand(0, 1)) ||
            (rand(0, 1) && rand(0, 1)) ||
            (rand(0, 1) && rand(0, 1)) ||
            (rand(0, 1) && rand(0, 1)) ||
            (rand(0, 1) && rand(0, 1))
        ) {
            return "RTL";
        }
    }

    return "LTR";
}