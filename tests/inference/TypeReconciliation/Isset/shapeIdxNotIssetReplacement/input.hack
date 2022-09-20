function foo(shape(?'a' => string) $arr): string {
    if (Shapes::idx($arr, 'a') === null) {
        return '';
    }

    return $arr['a'];
}

function bar(shape(?'a' => string) $arr): string {
    if (Shapes::idx($arr, 'a') is null) {
        return '';
    }

    return $arr['a'];
}