type str_t = shape('a' => string, 'b' => string)

function foo(shape('a' => arraykey, 'b' => string) $arr): void {
    if ($arr is str_t) {
        takes_string($arr['a']);
    } else {
        takes_int($arr['a']);
    }
}

function takes_string(string $s): void {}
function takes_int(int $s): void {}








