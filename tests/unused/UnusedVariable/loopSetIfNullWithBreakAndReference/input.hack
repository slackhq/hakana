$a = null;

while (rand(0, 1)) {
    if ($a !== null) {
        $a = 4;
        break;
    }

    $a = 5;
}

echo $a;