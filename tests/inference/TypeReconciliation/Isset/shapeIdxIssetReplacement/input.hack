function foo(shape(?'a' => string, ?'b' => ?int) $arr): arraykey {
    if (Shapes::idx($arr, 'a') is nonnull) {
        return $arr['a'];
    }

    if (Shapes::idx($arr, 'b') is nonnull) {
        echo $arr['a'];
        return $arr['a'];
    }

    return '';
}
