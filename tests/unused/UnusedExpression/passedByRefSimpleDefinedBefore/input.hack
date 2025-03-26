$a = vec[];
takes_ref(inout $a);

function takes_ref(inout ?vec<int> $p): void {
    $p = vec[0];
}