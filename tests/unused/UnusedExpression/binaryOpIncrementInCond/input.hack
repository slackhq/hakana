function foo(int $i, string $alias) : void {
    $i++;
    echo $i !== 0 ? $i : $alias;
    echo $i;
}
