function takesArray(dict<arraykey, mixed> $arr): void {
    foreach ($arr as $key => $_) {
        echo $key;
    }
}

takesArray(["good" => $_GET["bad"]]);