$a = 0;
while (rand(0, 1)) {
    echo $a;

    if (rand(0, 1) !== 0) {
        $a = 1;
    }

    continue;
}
