$a = vec[];
takes_ref($a);

function takes_ref(inout ?vec<int> $p): void {
    $p = vec[0];
}