$array = vec[
    vec[1, 2],
    vec[3, 4],
];

foreach ($array as list($a, $b)) {
    echo "A: $a; B: $b\n";
}