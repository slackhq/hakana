$a = 0;
while (rand(0, 1)) {
    if ($a < 20) {
        $a = $a + 1;
        echo "hello";
        continue;
    }
    echo "goodbye";
    break;
}