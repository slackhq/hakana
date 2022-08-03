$a = 0;

while (rand(0, 1)) {
    switch (rand(0, 1)) {
        default:
            echo $a;

            if (rand(0, 1)) {
                $a = 5;
                break;
            }
    }
}