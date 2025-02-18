$a = dict[
    "a" => vec[1],
    "b" => vec[2]
];

foreach (vec["a"] as $e){
    $b = $a[$e];
    takes_ref(inout $b);
    $a[$e] = $b;
}

function takes_ref(inout vec<arraykey> $p): void {
    echo implode(",", $p);
}