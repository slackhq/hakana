$a = false;

do {
    if (rand(0, 1)) {
        if (rand(0, 1)) {
            $a = true;
        }

        break;
    }
    $a = true;
}
while (rand(0,1));