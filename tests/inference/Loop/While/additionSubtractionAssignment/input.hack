$a = 0;

while (rand(0, 1)) {
    if (rand(0, 1) !== 0) {
        $a = $a + 1;
    } else if ($a !== 0) {
        $a = $a - 1;
    }
}
