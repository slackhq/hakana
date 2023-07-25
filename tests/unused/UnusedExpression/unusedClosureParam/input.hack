function foo(): void {
    $lst = vec[vec[1, 2], vec[3, 4]];
    Vec\map($lst, $chunk ==> do_op($lst));
}

function do_op(vec<mixed> $_lst): void {}