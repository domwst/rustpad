import {
  Box,
  Button,
  Container,
  Heading,
  Input,
  Stack,
  Text,
} from "@chakra-ui/react";
import { FormEvent, useState } from "react";

type NameGateProps = {
  darkMode: boolean;
  onSubmit: (name: string) => void;
};

function NameGate({ darkMode, onSubmit }: NameGateProps) {
  const [name, setName] = useState("");

  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    const trimmed = name.trim();
    if (trimmed) onSubmit(trimmed);
  }

  return (
    <Box
      minH="100vh"
      bgColor={darkMode ? "#1e1e1e" : "gray.50"}
      color={darkMode ? "#cbcaca" : "inherit"}
      display="grid"
      placeItems="center"
      px={4}
    >
      <Container
        as="form"
        maxW="sm"
        bgColor={darkMode ? "#252526" : "white"}
        borderWidth={1}
        borderColor={darkMode ? "#3c3c3c" : "gray.200"}
        rounded="lg"
        p={6}
        boxShadow="lg"
        onSubmit={handleSubmit}
      >
        <Stack spacing={4}>
          <Heading size="md">Enter your name</Heading>
          <Text fontSize="sm" color={darkMode ? "gray.300" : "gray.600"}>
            Your name will be shown next to your cursor during the interview and
            in the replay.
          </Text>
          <Input
            autoFocus
            maxLength={25}
            value={name}
            placeholder="Jane Doe"
            bgColor={darkMode ? "#333333" : "white"}
            onChange={(event) => setName(event.target.value)}
          />
          <Button colorScheme="blue" type="submit" isDisabled={!name.trim()}>
            Join room
          </Button>
        </Stack>
      </Container>
    </Box>
  );
}

export default NameGate;
