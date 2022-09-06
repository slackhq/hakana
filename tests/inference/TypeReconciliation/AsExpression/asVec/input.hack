function foo(mixed $m): void {
    $a = $m as vec<_>;
    foreach ($a as $b) {
        echo $b as string;
    }
}