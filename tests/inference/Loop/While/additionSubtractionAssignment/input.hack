$a = 0;

while (rand(0, 1)) {
    if (rand(0, 1)) {
        $a = $a + 1;
    } else if ($a) {
        $a = $a - 1;
    }
}