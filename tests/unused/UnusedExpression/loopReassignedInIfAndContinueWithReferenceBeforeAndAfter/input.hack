$a = 5;

echo $a;

while (rand(0, 1)) {
    if (rand(0, 1)) {
        $a = 7;
        continue;
    }

    $a = 3;
}

echo $a;