$a = rand(0,1) ? new \DateTime() : null;

switch(true) {
    case $a !== null && $a->format("Y") === "2020":
        $a->format("d-m-Y");
}