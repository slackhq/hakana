$a = 0;

while (rand(0, 1)) {
    if (rand(0, 1) !== 0) {
        $a++;
    } else if ($a !== 0) {
        $a--;
    }
}
