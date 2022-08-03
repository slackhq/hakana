function array_check(): void {
    $data = dict["f" =>  false];
    while (rand(0, 1) > 0 && !$data["f"]) {
        $data = dict["f" =>  true];
    }
}