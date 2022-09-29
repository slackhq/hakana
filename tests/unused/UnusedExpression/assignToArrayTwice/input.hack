function foo(string $c): void {
    $arr = vec[$c];
    $arr[] = 1;

    foreach ($arr as $e) {
        echo $e;
    }
}