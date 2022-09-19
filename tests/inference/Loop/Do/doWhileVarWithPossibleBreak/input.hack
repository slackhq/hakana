$a = false;

do {
    if (rand(0, 1)) {
        break;
    }
    if (rand(0, 1)) {
        $a = true;
        break;
    }
    $a = true;
}
while (rand(0,100) === 10);