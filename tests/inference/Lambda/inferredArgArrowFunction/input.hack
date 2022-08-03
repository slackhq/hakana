$bar = vec["foo", "bar"];

$bam = array_map(
    (string $a) ==> $a . "blah",
    $bar
);