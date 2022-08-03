$array = vec[1, 2, 3];
while (rand(1, 10) === 1) {
    $array[] = 4;
    $array[] = 5;
    $array[] = 6;
}

if (count($array) === 7) {}