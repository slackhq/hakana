function foo(int $i, string $alias) : void {
    $i++;
    echo $alias ?: $i;
    echo $i;
}
