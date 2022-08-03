$counter = 0;

while (rand(0, 1)) {
    if ($counter > 0) {
        $counter = $counter - 1;
    } else {
        $counter = $counter + 1;
    }
}