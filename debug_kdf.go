package main

import (
	"crypto/aes"
	"crypto/cipher"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"fmt"
	"io"
	"golang.org/x/crypto/hkdf"
	"golang.org/x/crypto/scrypt"
)

func main() {
	password := []byte("123456123")
	saltB64 := "IajRu/RqU/v8LG7xDc8ARD/pUo+x4XeDESy2JcmH3Xs="
	encKeyB64 := "vPlK2K0mTbm5uTQ3iA1ZV2sjOADUaChwiSqo8YreWaLC935IfJepbAWvr2kZvD89ce2FN0y374zxIPo92PVWaw=="

	salt, _ := base64.StdEncoding.DecodeString(saltB64)
	encKey, _ := base64.StdEncoding.DecodeString(encKeyB64)

	fmt.Printf("password: %s\n", password)
	fmt.Printf("salt (%d bytes): %s\n", len(salt), hex.EncodeToString(salt))
	fmt.Printf("encKey (%d bytes): %s\n", len(encKey), hex.EncodeToString(encKey))

	// Step 1: scrypt
	scryptKey, err := scrypt.Key(password, salt, 65536, 8, 1, 32)
	if err != nil {
		panic(err)
	}
	fmt.Printf("\n--- Step 1: scrypt ---\n")
	fmt.Printf("scryptKey (%d bytes): %s\n", len(scryptKey), hex.EncodeToString(scryptKey))

	// Step 2: HKDF to derive GCM key (because HKDF flag is set)
	hkdfR := hkdf.New(sha256.New, scryptKey, nil, []byte("AES-GCM file content encryption"))
	gcmKey := make([]byte, 32)
	io.ReadFull(hkdfR, gcmKey)
	fmt.Printf("\n--- Step 2: HKDF(scryptKey, 'AES-GCM file content encryption') ---\n")
	fmt.Printf("gcmKey (%d bytes): %s\n", len(gcmKey), hex.EncodeToString(gcmKey))

	// Step 3: Split encKey into nonce + ciphertext
	ivLen := 16 // GCMIV128
	nonce := encKey[:ivLen]
	ciphertext := encKey[ivLen:]
	fmt.Printf("\n--- Step 3: Split encrypted key ---\n")
	fmt.Printf("nonce (%d bytes): %s\n", len(nonce), hex.EncodeToString(nonce))
	fmt.Printf("ciphertext (%d bytes): %s\n", len(ciphertext), hex.EncodeToString(ciphertext))

	// Step 4: AES-GCM decrypt with 16-byte nonce
	block, err := aes.NewCipher(gcmKey)
	if err != nil {
		panic(err)
	}
	aead, err := cipher.NewGCMWithNonceSize(block, 16)
	if err != nil {
		panic(err)
	}

	// AAD = blockNo(0) as big-endian uint64 = 8 zero bytes
	aad := make([]byte, 8)
	fmt.Printf("aad (%d bytes): %s\n", len(aad), hex.EncodeToString(aad))

	masterKey, err := aead.Open(nil, nonce, ciphertext, aad)
	if err != nil {
		fmt.Printf("\n!!! DECRYPTION FAILED: %v\n", err)
		return
	}
	fmt.Printf("\n--- Step 4: Decrypted master key ---\n")
	fmt.Printf("masterKey (%d bytes): %s\n", len(masterKey), hex.EncodeToString(masterKey))

	// Format as gocryptfs display format
	fmt.Printf("\nFormatted: ")
	for i := 0; i < 32; i += 4 {
		if i > 0 {
			fmt.Printf("-")
		}
		fmt.Printf("%s", hex.EncodeToString(masterKey[i:i+4]))
	}
	fmt.Println()
}
