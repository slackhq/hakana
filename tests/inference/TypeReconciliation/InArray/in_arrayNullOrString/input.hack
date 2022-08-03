function test(?string $x, string $y): void {
    if (in_array($x, vec[null, $y], true)) {
        if ($x === null) {
            echo "Saw null\n";
        }
        echo "Saw $x\n";
    }
}