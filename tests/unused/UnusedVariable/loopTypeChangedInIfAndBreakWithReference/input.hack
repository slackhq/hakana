$a = 1;

while (rand(0, 1)) {
    if (rand(0, 1)) {
        $a = 2;
        break;
    }

    $a = 3;
}

echo $a;