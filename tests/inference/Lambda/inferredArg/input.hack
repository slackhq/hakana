$bar = vec["foo", "bar"];

$bam = array_map(
    function(string $a) {
        return $a . "blah";
    },
    $bar
);