$a = vec[1, 2, 3];

$b = array_map(
    function(int $i) {
        return $i * 3;
    },
    $a
);

foreach ($b as $c) {
    echo $c;
}