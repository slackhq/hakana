if (rand(0,100) === 10) {
    $badge = "hello";
}
else {
    throw new \Exception();
}

echo $badge;