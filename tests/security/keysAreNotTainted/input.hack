function takesArray(dict<arraykey, mixed> $arr): void {
    foreach ($arr as $key => $_) {
        echo $key;
    }
}

takesArray(dict["good" => $_GET["bad"]]);