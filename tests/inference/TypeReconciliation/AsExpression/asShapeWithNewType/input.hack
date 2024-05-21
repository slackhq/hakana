function foo(shape('a' => id_t, ?'b' => int) $arr): void {
    bar($arr as shape('a' => string));
}

function bar(shape('a' => string) $arr): void {}