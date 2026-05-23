import {
  Box,
  Button,
  ButtonGroup,
  HStack,
  Icon,
  Input,
  Popover,
  PopoverArrow,
  PopoverBody,
  PopoverCloseButton,
  PopoverContent,
  PopoverFooter,
  PopoverHeader,
  PopoverTrigger,
  Text,
  useDisclosure,
} from "@chakra-ui/react";
import { useRef } from "react";
import { FaPalette } from "react-icons/fa";
import { VscAccount } from "react-icons/vsc";

import { UserInfo } from "./rustpad";
import { USER_COLOR_HUES } from "./userColors";

type UserProps = {
  info: UserInfo;
  isMe?: boolean;
  onChangeName?: (name: string) => void;
  onChangeColor?: (hue: number) => void;
  darkMode: boolean;
};

function User({
  info,
  isMe = false,
  onChangeName,
  onChangeColor,
  darkMode,
}: UserProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const { isOpen, onOpen, onClose } = useDisclosure();

  const nameColor = `hsl(${info.hue}, 90%, ${darkMode ? "70%" : "25%"})`;
  return (
    <Popover
      placement="right"
      isOpen={isOpen}
      onClose={onClose}
      initialFocusRef={inputRef}
    >
      <PopoverTrigger>
        <HStack
          p={2}
          rounded="md"
          _hover={{
            bgColor: darkMode ? "#464647" : "gray.200",
            cursor: "pointer",
          }}
          onClick={() => isMe && onOpen()}
        >
          <Icon as={VscAccount} />
          <Text fontWeight="medium" color={nameColor}>
            {info.name}
          </Text>
          {isMe && <Text>(you)</Text>}
        </HStack>
      </PopoverTrigger>
      <PopoverContent
        bgColor={darkMode ? "#333333" : "white"}
        borderColor={darkMode ? "#464647" : "gray.200"}
      >
        <PopoverHeader
          fontWeight="semibold"
          borderColor={darkMode ? "#464647" : "gray.200"}
        >
          Update Info
        </PopoverHeader>
        <PopoverArrow bgColor={darkMode ? "#333333" : "white"} />
        <PopoverCloseButton />
        <PopoverBody borderColor={darkMode ? "#464647" : "gray.200"}>
          <Input
            ref={inputRef}
            mb={2}
            value={info.name}
            maxLength={25}
            onChange={(event) => onChangeName?.(event.target.value)}
          />
          <HStack spacing={1.5} align="center" flexWrap="wrap">
            <HStack spacing={1} w="full" color={darkMode ? "gray.300" : "gray.600"}>
              <Icon as={FaPalette} />
              <Text fontSize="sm" fontWeight="medium">
                Color
              </Text>
            </HStack>
            {USER_COLOR_HUES.map((hue) => {
              const selected = hue === info.hue;
              return (
                <Box
                  as="button"
                  key={hue}
                  type="button"
                  aria-label={`Choose color ${hue}`}
                  title={`Choose color ${hue}`}
                  w={6}
                  h={6}
                  rounded="full"
                  bgColor={`hsl(${hue}, 80%, 45%)`}
                  border="2px solid"
                  borderColor={selected ? "blue.400" : darkMode ? "gray.600" : "white"}
                  boxShadow={selected ? "0 0 0 2px var(--chakra-colors-blue-400)" : "sm"}
                  _hover={{ transform: "scale(1.08)" }}
                  onClick={() => onChangeColor?.(hue)}
                />
              );
            })}
          </HStack>
        </PopoverBody>
        <PopoverFooter
          display="flex"
          justifyContent="flex-end"
          borderColor={darkMode ? "#464647" : "gray.200"}
        >
          <ButtonGroup size="sm">
            <Button colorScheme="blue" onClick={onClose}>
              Done
            </Button>
          </ButtonGroup>
        </PopoverFooter>
      </PopoverContent>
    </Popover>
  );
}

export default User;
