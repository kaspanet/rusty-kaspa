# Source: https://github.com/dot-asm/cryptogams/blob/master/x86_64/keccak1600-x86_64.pl

.text

.def	__KeccakF1600;	.scl 3;	.type 32;	.endef
.p2align	5
__KeccakF1600:
	.byte	0xf3,0x0f,0x1e,0xfa

	movq	60(%rdi),%rax
	movq	68(%rdi),%rbx
	movq	76(%rdi),%rcx
	movq	84(%rdi),%rdx
	movq	92(%rdi),%rbp
	jmp	.Loop

.p2align	5
.Loop:
	movq	-100(%rdi),%r8
	movq	-52(%rdi),%r9
	movq	-4(%rdi),%r10
	movq	44(%rdi),%r11

	xorq	-84(%rdi),%rcx
	xorq	-76(%rdi),%rdx
	xorq	%r8,%rax
	xorq	-92(%rdi),%rbx
	xorq	-44(%rdi),%rcx
	xorq	-60(%rdi),%rax
	movq	%rbp,%r12
	xorq	-68(%rdi),%rbp

	xorq	%r10,%rcx
	xorq	-20(%rdi),%rax
	xorq	-36(%rdi),%rdx
	xorq	%r9,%rbx
	xorq	-28(%rdi),%rbp

	xorq	36(%rdi),%rcx
	xorq	20(%rdi),%rax
	xorq	4(%rdi),%rdx
	xorq	-12(%rdi),%rbx
	xorq	12(%rdi),%rbp

	movq	%rcx,%r13
	rolq	$1,%rcx
	xorq	%rax,%rcx
	xorq	%r11,%rdx

	rolq	$1,%rax
	xorq	%rdx,%rax
	xorq	28(%rdi),%rbx

	rolq	$1,%rdx
	xorq	%rbx,%rdx
	xorq	52(%rdi),%rbp

	rolq	$1,%rbx
	xorq	%rbp,%rbx

	rolq	$1,%rbp
	xorq	%r13,%rbp
	xorq	%rcx,%r9
	xorq	%rdx,%r10
	rolq	$44,%r9
	xorq	%rbp,%r11
	xorq	%rax,%r12
	rolq	$43,%r10
	xorq	%rbx,%r8
	movq	%r9,%r13
	rolq	$21,%r11
	orq	%r10,%r9
	xorq	%r8,%r9
	rolq	$14,%r12

	xorq	(%r15),%r9
	leaq	8(%r15),%r15

	movq	%r12,%r14
	andq	%r11,%r12
	movq	%r9,-100(%rsi)
	xorq	%r10,%r12
	notq	%r10
	movq	%r12,-84(%rsi)

	orq	%r11,%r10
	movq	76(%rdi),%r12
	xorq	%r13,%r10
	movq	%r10,-92(%rsi)

	andq	%r8,%r13
	movq	-28(%rdi),%r9
	xorq	%r14,%r13
	movq	-20(%rdi),%r10
	movq	%r13,-68(%rsi)

	orq	%r8,%r14
	movq	-76(%rdi),%r8
	xorq	%r11,%r14
	movq	28(%rdi),%r11
	movq	%r14,-76(%rsi)


	xorq	%rbp,%r8
	xorq	%rdx,%r12
	rolq	$28,%r8
	xorq	%rcx,%r11
	xorq	%rax,%r9
	rolq	$61,%r12
	rolq	$45,%r11
	xorq	%rbx,%r10
	rolq	$20,%r9
	movq	%r8,%r13
	orq	%r12,%r8
	rolq	$3,%r10

	xorq	%r11,%r8
	movq	%r8,-36(%rsi)

	movq	%r9,%r14
	andq	%r13,%r9
	movq	-92(%rdi),%r8
	xorq	%r12,%r9
	notq	%r12
	movq	%r9,-28(%rsi)

	orq	%r11,%r12
	movq	-44(%rdi),%r9
	xorq	%r10,%r12
	movq	%r12,-44(%rsi)

	andq	%r10,%r11
	movq	60(%rdi),%r12
	xorq	%r14,%r11
	movq	%r11,-52(%rsi)

	orq	%r10,%r14
	movq	4(%rdi),%r10
	xorq	%r13,%r14
	movq	52(%rdi),%r11
	movq	%r14,-60(%rsi)


	xorq	%rbp,%r10
	xorq	%rax,%r11
	rolq	$25,%r10
	xorq	%rdx,%r9
	rolq	$8,%r11
	xorq	%rbx,%r12
	rolq	$6,%r9
	xorq	%rcx,%r8
	rolq	$18,%r12
	movq	%r10,%r13
	andq	%r11,%r10
	rolq	$1,%r8

	notq	%r11
	xorq	%r9,%r10
	movq	%r10,-12(%rsi)

	movq	%r12,%r14
	andq	%r11,%r12
	movq	-12(%rdi),%r10
	xorq	%r13,%r12
	movq	%r12,-4(%rsi)

	orq	%r9,%r13
	movq	84(%rdi),%r12
	xorq	%r8,%r13
	movq	%r13,-20(%rsi)

	andq	%r8,%r9
	xorq	%r14,%r9
	movq	%r9,12(%rsi)

	orq	%r8,%r14
	movq	-60(%rdi),%r9
	xorq	%r11,%r14
	movq	36(%rdi),%r11
	movq	%r14,4(%rsi)


	movq	-68(%rdi),%r8

	xorq	%rcx,%r10
	xorq	%rdx,%r11
	rolq	$10,%r10
	xorq	%rbx,%r9
	rolq	$15,%r11
	xorq	%rbp,%r12
	rolq	$36,%r9
	xorq	%rax,%r8
	rolq	$56,%r12
	movq	%r10,%r13
	orq	%r11,%r10
	rolq	$27,%r8

	notq	%r11
	xorq	%r9,%r10
	movq	%r10,28(%rsi)

	movq	%r12,%r14
	orq	%r11,%r12
	xorq	%r13,%r12
	movq	%r12,36(%rsi)

	andq	%r9,%r13
	xorq	%r8,%r13
	movq	%r13,20(%rsi)

	orq	%r8,%r9
	xorq	%r14,%r9
	movq	%r9,52(%rsi)

	andq	%r14,%r8
	xorq	%r11,%r8
	movq	%r8,44(%rsi)


	xorq	-84(%rdi),%rdx
	xorq	-36(%rdi),%rbp
	rolq	$62,%rdx
	xorq	68(%rdi),%rcx
	rolq	$55,%rbp
	xorq	12(%rdi),%rax
	rolq	$2,%rcx
	xorq	20(%rdi),%rbx
	xchgq	%rsi,%rdi
	rolq	$39,%rax
	rolq	$41,%rbx
	movq	%rdx,%r13
	andq	%rbp,%rdx
	notq	%rbp
	xorq	%rcx,%rdx
	movq	%rdx,92(%rdi)

	movq	%rax,%r14
	andq	%rbp,%rax
	xorq	%r13,%rax
	movq	%rax,60(%rdi)

	orq	%rcx,%r13
	xorq	%rbx,%r13
	movq	%r13,84(%rdi)

	andq	%rbx,%rcx
	xorq	%r14,%rcx
	movq	%rcx,76(%rdi)

	orq	%r14,%rbx
	xorq	%rbp,%rbx
	movq	%rbx,68(%rdi)

	movq	%rdx,%rbp
	movq	%r13,%rdx

	testq	$255,%r15
	jnz	.Loop

	leaq	-192(%r15),%r15
	.byte	0xf3,0xc3


.globl	KeccakF1600
.def	KeccakF1600;	.scl 2;	.type 32;	.endef
.p2align	5
KeccakF1600:
	.byte	0xf3,0x0f,0x1e,0xfa
	movq	%rdi,8(%rsp)
	movq	%rsi,16(%rsp)
	movq	%rsp,%r11
.LSEH_begin_KeccakF1600:


	movq	%rcx,%rdi
	pushq	%rbx

	pushq	%rbp

	pushq	%r12

	pushq	%r13

	pushq	%r14

	pushq	%r15


	leaq	100(%rdi),%rdi
	subq	$200,%rsp

.LSEH_body_KeccakF1600:


	notq	-92(%rdi)
	notq	-84(%rdi)
	notq	-36(%rdi)
	notq	-4(%rdi)
	notq	36(%rdi)
	notq	60(%rdi)

	leaq	iotas(%rip),%r15
	leaq	100(%rsp),%rsi

	call	__KeccakF1600

	notq	-92(%rdi)
	notq	-84(%rdi)
	notq	-36(%rdi)
	notq	-4(%rdi)
	notq	36(%rdi)
	notq	60(%rdi)
	leaq	-100(%rdi),%rdi

	leaq	248(%rsp),%r11

	movq	-48(%r11),%r15
	movq	-40(%r11),%r14
	movq	-32(%r11),%r13
	movq	-24(%r11),%r12
	movq	-16(%r11),%rbp
	movq	-8(%r11),%rbx
	leaq	(%r11),%rsp
.LSEH_epilogue_KeccakF1600:
	mov	8(%r11),%rdi
	mov	16(%r11),%rsi

	.byte	0xf3,0xc3

.LSEH_end_KeccakF1600:
.globl	SHA3_absorb
.def	SHA3_absorb;	.scl 2;	.type 32;	.endef
.p2align	5
SHA3_absorb:
	.byte	0xf3,0x0f,0x1e,0xfa
	movq	%rdi,8(%rsp)
	movq	%rsi,16(%rsp)
	movq	%rsp,%r11
.LSEH_begin_SHA3_absorb:


	movq	%rcx,%rdi
	movq	%rdx,%rsi
	movq	%r8,%rdx
	movq	%r9,%rcx
	pushq	%rbx

	pushq	%rbp

	pushq	%r12

	pushq	%r13

	pushq	%r14

	pushq	%r15


	leaq	100(%rdi),%rdi
	subq	$232,%rsp

.LSEH_body_SHA3_absorb:


	movq	%rsi,%r9
	leaq	100(%rsp),%rsi

	notq	-92(%rdi)
	notq	-84(%rdi)
	notq	-36(%rdi)
	notq	-4(%rdi)
	notq	36(%rdi)
	notq	60(%rdi)
	leaq	iotas(%rip),%r15

	movq	%rcx,216-100(%rsi)

.Loop_absorb:
	cmpq	%rcx,%rdx
	jc	.Ldone_absorb

	shrq	$3,%rcx
	leaq	-100(%rdi),%r8

.Lblock_absorb:
	movq	(%r9),%rax
	leaq	8(%r9),%r9
	xorq	(%r8),%rax
	leaq	8(%r8),%r8
	subq	$8,%rdx
	movq	%rax,-8(%r8)
	subq	$1,%rcx
	jnz	.Lblock_absorb

	movq	%r9,200-100(%rsi)
	movq	%rdx,208-100(%rsi)
	call	__KeccakF1600
	movq	200-100(%rsi),%r9
	movq	208-100(%rsi),%rdx
	movq	216-100(%rsi),%rcx
	jmp	.Loop_absorb

.p2align	5
.Ldone_absorb:
	movq	%rdx,%rax

	notq	-92(%rdi)
	notq	-84(%rdi)
	notq	-36(%rdi)
	notq	-4(%rdi)
	notq	36(%rdi)
	notq	60(%rdi)

	leaq	280(%rsp),%r11

	movq	-48(%r11),%r15
	movq	-40(%r11),%r14
	movq	-32(%r11),%r13
	movq	-24(%r11),%r12
	movq	-16(%r11),%rbp
	movq	-8(%r11),%rbx
	leaq	(%r11),%rsp
.LSEH_epilogue_SHA3_absorb:
	mov	8(%r11),%rdi
	mov	16(%r11),%rsi

	.byte	0xf3,0xc3

.LSEH_end_SHA3_absorb:
.globl	SHA3_squeeze
.def	SHA3_squeeze;	.scl 2;	.type 32;	.endef
.p2align	5
SHA3_squeeze:
	.byte	0xf3,0x0f,0x1e,0xfa
	movq	%rdi,8(%rsp)
	movq	%rsi,16(%rsp)
	movq	%rsp,%r11
.LSEH_begin_SHA3_squeeze:


	movq	%rcx,%rdi
	movq	%rdx,%rsi
	movq	%r8,%rdx
	movq	%r9,%rcx
	pushq	%r12

	pushq	%r13

	pushq	%r14

	subq	$32,%rsp

.LSEH_body_SHA3_squeeze:


	shrq	$3,%rcx
	movq	%rdi,%r8
	movq	%rsi,%r12
	movq	%rdx,%r13
	movq	%rcx,%r14
	jmp	.Loop_squeeze

.p2align	5
.Loop_squeeze:
	cmpq	$8,%r13
	jb	.Ltail_squeeze

	movq	(%r8),%rax
	leaq	8(%r8),%r8
	movq	%rax,(%r12)
	leaq	8(%r12),%r12
	subq	$8,%r13
	jz	.Ldone_squeeze

	subq	$1,%rcx
	jnz	.Loop_squeeze

	movq	%rdi,%rcx
	call	KeccakF1600
	movq	%rdi,%r8
	movq	%r14,%rcx
	jmp	.Loop_squeeze

.Ltail_squeeze:
	movq	%r8,%rsi
	movq	%r12,%rdi
	movq	%r13,%rcx
.byte	0xf3,0xa4

.Ldone_squeeze:
	movq	32(%rsp),%r14
	movq	40(%rsp),%r13
	movq	48(%rsp),%r12
	addq	$56,%rsp

.LSEH_epilogue_SHA3_squeeze:
	mov	8(%rsp),%rdi
	mov	16(%rsp),%rsi

	.byte	0xf3,0xc3

.LSEH_end_SHA3_squeeze:
.p2align	8
.quad	0,0,0,0,0,0,0,0

iotas:
.quad	0x0000000000000001
.quad	0x0000000000008082
.quad	0x800000000000808a
.quad	0x8000000080008000
.quad	0x000000000000808b
.quad	0x0000000080000001
.quad	0x8000000080008081
.quad	0x8000000000008009
.quad	0x000000000000008a
.quad	0x0000000000000088
.quad	0x0000000080008009
.quad	0x000000008000000a
.quad	0x000000008000808b
.quad	0x800000000000008b
.quad	0x8000000000008089
.quad	0x8000000000008003
.quad	0x8000000000008002
.quad	0x8000000000000080
.quad	0x000000000000800a
.quad	0x800000008000000a
.quad	0x8000000080008081
.quad	0x8000000000008080
.quad	0x0000000080000001
.quad	0x8000000080008008

.byte	75,101,99,99,97,107,45,49,54,48,48,32,97,98,115,111,114,98,32,97,110,100,32,115,113,117,101,101,122,101,32,102,111,114,32,120,56,54,95,54,52,44,32,67,82,89,80,84,79,71,65,77,83,32,98,121,32,60,97,112,112,114,111,64,111,112,101,110,115,115,108,46,111,114,103,62,0
.section	.pdata
.p2align	2
.rva	.LSEH_begin_KeccakF1600
.rva	.LSEH_body_KeccakF1600
.rva	.LSEH_info_KeccakF1600_prologue

.rva	.LSEH_body_KeccakF1600
.rva	.LSEH_epilogue_KeccakF1600
.rva	.LSEH_info_KeccakF1600_body

.rva	.LSEH_epilogue_KeccakF1600
.rva	.LSEH_end_KeccakF1600
.rva	.LSEH_info_KeccakF1600_epilogue

.rva	.LSEH_begin_SHA3_absorb
.rva	.LSEH_body_SHA3_absorb
.rva	.LSEH_info_SHA3_absorb_prologue

.rva	.LSEH_body_SHA3_absorb
.rva	.LSEH_epilogue_SHA3_absorb
.rva	.LSEH_info_SHA3_absorb_body

.rva	.LSEH_epilogue_SHA3_absorb
.rva	.LSEH_end_SHA3_absorb
.rva	.LSEH_info_SHA3_absorb_epilogue

.rva	.LSEH_begin_SHA3_squeeze
.rva	.LSEH_body_SHA3_squeeze
.rva	.LSEH_info_SHA3_squeeze_prologue

.rva	.LSEH_body_SHA3_squeeze
.rva	.LSEH_epilogue_SHA3_squeeze
.rva	.LSEH_info_SHA3_squeeze_body

.rva	.LSEH_epilogue_SHA3_squeeze
.rva	.LSEH_end_SHA3_squeeze
.rva	.LSEH_info_SHA3_squeeze_epilogue

.section	.xdata
.p2align	3
.LSEH_info_KeccakF1600_prologue:
.byte	1,0,5,0x0b
.byte	0,0x74,1,0
.byte	0,0x64,2,0
.byte	0,0xb3
.byte	0,0
.long	0,0
.LSEH_info_KeccakF1600_body:
.byte	1,0,18,0
.byte	0x00,0xf4,0x19,0x00
.byte	0x00,0xe4,0x1a,0x00
.byte	0x00,0xd4,0x1b,0x00
.byte	0x00,0xc4,0x1c,0x00
.byte	0x00,0x54,0x1d,0x00
.byte	0x00,0x34,0x1e,0x00
.byte	0x00,0x74,0x20,0x00
.byte	0x00,0x64,0x21,0x00
.byte	0x00,0x01,0x1f,0x00
.byte	0x00,0x00,0x00,0x00
.byte	0x00,0x00,0x00,0x00
.LSEH_info_KeccakF1600_epilogue:
.byte	1,0,5,11
.byte	0x00,0x74,0x01,0x00
.byte	0x00,0x64,0x02,0x00
.byte	0x00,0xb3
.byte	0x00,0x00,0x00,0x00,0x00,0x00
.byte	0x00,0x00,0x00,0x00

.LSEH_info_SHA3_absorb_prologue:
.byte	1,0,5,0x0b
.byte	0,0x74,1,0
.byte	0,0x64,2,0
.byte	0,0xb3
.byte	0,0
.long	0,0
.LSEH_info_SHA3_absorb_body:
.byte	1,0,18,0
.byte	0x00,0xf4,0x1d,0x00
.byte	0x00,0xe4,0x1e,0x00
.byte	0x00,0xd4,0x1f,0x00
.byte	0x00,0xc4,0x20,0x00
.byte	0x00,0x54,0x21,0x00
.byte	0x00,0x34,0x22,0x00
.byte	0x00,0x74,0x24,0x00
.byte	0x00,0x64,0x25,0x00
.byte	0x00,0x01,0x23,0x00
.byte	0x00,0x00,0x00,0x00
.byte	0x00,0x00,0x00,0x00
.LSEH_info_SHA3_absorb_epilogue:
.byte	1,0,5,11
.byte	0x00,0x74,0x01,0x00
.byte	0x00,0x64,0x02,0x00
.byte	0x00,0xb3
.byte	0x00,0x00,0x00,0x00,0x00,0x00
.byte	0x00,0x00,0x00,0x00

.LSEH_info_SHA3_squeeze_prologue:
.byte	1,0,5,0x0b
.byte	0,0x74,1,0
.byte	0,0x64,2,0
.byte	0,0xb3
.byte	0,0
.long	0,0
.LSEH_info_SHA3_squeeze_body:
.byte	1,0,11,0
.byte	0x00,0xe4,0x04,0x00
.byte	0x00,0xd4,0x05,0x00
.byte	0x00,0xc4,0x06,0x00
.byte	0x00,0x74,0x08,0x00
.byte	0x00,0x64,0x09,0x00
.byte	0x00,0x62
.byte	0x00,0x00,0x00,0x00,0x00,0x00
.LSEH_info_SHA3_squeeze_epilogue:
.byte	1,0,4,0
.byte	0x00,0x74,0x01,0x00
.byte	0x00,0x64,0x02,0x00
.byte	0x00,0x00,0x00,0x00

