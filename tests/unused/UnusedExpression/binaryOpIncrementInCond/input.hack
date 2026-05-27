function foo(int $i, string $alias) : void {
    $i++;
    echo $i ?: $alias;
    echo $i;
}
