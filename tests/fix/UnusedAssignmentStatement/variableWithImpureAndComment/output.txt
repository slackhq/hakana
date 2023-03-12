function foo(): void {
    /* HHAST_FIXME[UnusedVariable] */ $a = rand(0, 1);
    $b = 0;
    echo $b;
}
