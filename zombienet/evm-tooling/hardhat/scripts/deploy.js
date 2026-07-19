// Deploy the sample Token to the numen network.
//
//   npx hardhat --network numen run scripts/deploy.js

import { network } from "hardhat";

const { ethers } = await network.create("numen");

const initialSupply = ethers.parseUnits("1000000", 18);
const Token = await ethers.getContractFactory("Token");
const token = await Token.deploy(initialSupply);
await token.waitForDeployment();

const addr = await token.getAddress();
const [deployer] = await ethers.getSigners();
console.log(`Deployer: ${deployer.address}`);
console.log(`Token:    ${addr}`);
console.log(`Supply:   ${await token.totalSupply()}`);
