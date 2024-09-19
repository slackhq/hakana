function takesArray(dict<arraykey, mixed> $arr): void {
    foreach ($arr as $key => $_) {
        echo $key;
    }
}

takesArray(dict["good" => HH\global_get('_GET')["bad"]]);