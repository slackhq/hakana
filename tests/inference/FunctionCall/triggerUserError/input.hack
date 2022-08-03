function mightLeave() : string {
    if (rand(0, 1)) {
        trigger_error("bad", E_USER_ERROR);
    } else {
        return "here";
    }
}