final class A {
    public function deleteUser(AsyncMysqlConnection $conn) : void {
        $userId = (string) HH\global_get('_GET')["user_id"];
        $conn->query("delete from users where user_id = " . $userId);
    }
}