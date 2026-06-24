$depth = 0;
$position = 0;
while ($depth === 0) {
    if (rand(0, 1) !== 0) {
        $depth++;
    } else if (rand(0, 1) !== 0) {
        $depth--;
    }
    $position++;
}
