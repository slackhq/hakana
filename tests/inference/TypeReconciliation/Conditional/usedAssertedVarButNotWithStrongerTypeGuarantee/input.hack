function broken(bool $b, ?User $u) : void {
    if ($b || (rand(0, 1) && (!$u || takesUser($u)))) {
        return;
    }

    if ($u) {}
}

class User {}

function takesUser(User $a) : bool {
    return true;
}