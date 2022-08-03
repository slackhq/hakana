function foo(vec<string> $arr): vec<string> {
    if ($arr is KeyedContainer<_, _>) {
        return $arr;
    }
    
    return vec[];
}