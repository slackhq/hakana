$a = 5;

while (rand(0, 1)) {
    if (rand(0, 1) !== 0) {
        $a = 7;
        continue;
    }

    $a = 3;
}

echo $a;
