$a = 0;

while (rand(0, 1)) {
    if (rand(0, 1)) {
        $a++;
    } else if ($a) {
        $a--;
    }
}