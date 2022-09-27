enum A: string {
    B = 'b';
    C = 'c';
    D = 'd';
    E = 'e';
}

function bar(string $s): void {
    if ($s === A::B) {
        echo $s;
    }
}