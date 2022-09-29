$a = null;

while (rand(0, 1)) {
    if ($a !== null) {
        $a = 4;
        continue;
    }

    $a = 5;
}

echo $a;