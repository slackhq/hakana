$data = dict["a" =>  false];
while (!$data["a"]) {
    if (rand() % 2 > 0) {
        $data = dict["a" =>  true];
    }
}