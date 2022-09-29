$a = dict[
    "a" => vec[1],
    "b" => vec[2]
];

foreach (vec["a"] as $e){
    takes_ref($a[$e]);
}

function takes_ref(inout vec<arraykey> $p): void {
    echo implode(",", $p);
}