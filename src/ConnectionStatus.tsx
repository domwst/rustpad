import { HStack, Icon, Text } from "@chakra-ui/react";
import { VscCircleFilled } from "react-icons/vsc";

type ConnectionStatusProps = {
  connection: "connected" | "disconnected" | "desynchronized" | "closed";
  darkMode: boolean;
};

function ConnectionStatus({ connection, darkMode }: ConnectionStatusProps) {
  return (
    <HStack spacing={1}>
      <Icon
        as={VscCircleFilled}
        color={
          {
            connected: "green.500",
            disconnected: "orange.500",
            desynchronized: "red.500",
            closed: "purple.500",
          }[connection]
        }
      />
      <Text
        fontSize="sm"
        fontStyle="italic"
        color={darkMode ? "gray.300" : "gray.600"}
      >
        {
          {
            connected: "You are connected!",
            disconnected: "Connecting to the server...",
            desynchronized: "Disconnected, please refresh.",
            closed: "Room stopped. Replay is available.",
          }[connection]
        }
      </Text>
    </HStack>
  );
}

export default ConnectionStatus;
