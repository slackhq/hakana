$a = 0;
for ($i = 0; $i < 1000; $i++) {
    if (rand(0, 1)) {
        $a = $a + 1;
        continue;
    }
    break;
}

echo $a;