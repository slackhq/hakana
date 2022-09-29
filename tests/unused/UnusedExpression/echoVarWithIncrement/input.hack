function foo(int $i) : void {
    echo $i;
}

$a = 5;

while (rand(0, 1)) {
    foo(++$a);
}