function foo(): void {
    $arr = dict[
        0 => "a",
        1 => "b",
        2 => "c",
    ];

    foreach ($arr as $k => $v) {
        takesInt($k);
        takesString($v);
    }
}

function takesInt(int $i): void {}
function takesString(string $s): void {}