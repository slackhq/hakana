function foo(int $i): void {
    $a = (int $b) ==> $b;
    $c = $a($i);
    echo $c;
}