$depth = 0;
$position = 0;
while (!$depth) {
    if (rand(0, 1)) {
        $depth++;
    } else if (rand(0, 1)) {
        $depth--;
    }
    $position++;
}