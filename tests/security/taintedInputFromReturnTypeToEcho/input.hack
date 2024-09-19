final class A {
    public function getUserId() : string {
        return (string) HH\global_get('_GET')["user_id"];
    }

    public function getAppendedUserId() : string {
        return "aaaa" . $this->getUserId();
    }

    public function deleteUser(AsyncMysqlConnection $conn) : void {
        $userId = $this->getAppendedUserId();
        echo $userId;
    }
}